param(
  [string]$Sysroot = 'build/tools/wasi-34/wasi-sysroot-34.0',
  [string]$Output = 'build/mutation-sdk-wasm',
  [string]$Evidence = 'build/mutation-guest-recovery',
  [string]$Python = 'python'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Mutation recovery qualification currently requires Windows.'
}
$repo = Split-Path $PSScriptRoot -Parent
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Recovery qualification failed: $Program (exit $LASTEXITCODE)" }
}
function TestBinary([string]$Features) {
  $messages = & cargo test --release --locked --offline --manifest-path plugin_runtime/Cargo.toml --features $Features --test mutation_owner --no-run --message-format=json
  if ($LASTEXITCODE -ne 0) { throw "Cannot build $Features test host." }
  $executables = @($messages | ForEach-Object {
    $entry = $_ | ConvertFrom-Json
    if ($entry.reason -eq 'compiler-artifact' -and $entry.target.name -eq 'mutation_owner' -and $entry.executable) {
      $entry.executable
    }
  })
  if ($executables.Count -ne 1) { throw "Expected one $Features test host, found $($executables.Count)." }
  return (Resolve-Path -LiteralPath $executables[0]).Path
}
function RunTests([string]$Program, [string]$Filter, [string[]]$Expected, [string]$Log) {
  $listed = & $Program $Filter --list
  if ($LASTEXITCODE -ne 0) { throw "Cannot enumerate $Filter." }
  $actual = @($listed | Where-Object { $_ -match ': test$' } | Sort-Object)
  $wanted = @($Expected | ForEach-Object { "${_}: test" } | Sort-Object)
  if ($actual.Count -ne $wanted.Count -or (Compare-Object $wanted $actual)) {
    throw "Unexpected test inventory for ${Filter}: $($actual -join ', ')"
  }
  & $Program $Filter --test-threads=1 --nocapture 2>&1 | Tee-Object -FilePath $Log
  if ($LASTEXITCODE -ne 0) { throw "Failed $Filter; see $Log." }
  $requiredResult = 'test result: ok. ' + $Expected.Count + ' passed; 0 failed; 0 ignored;'
  if (-not (Get-Content -LiteralPath $Log -Raw).Contains($requiredResult)) {
    throw "Expected $($Expected.Count) executed, passing tests for $Filter; ignored or zero execution is not qualification."
  }
}

$variables = @('MORROW_MUTATION_SDK_WASM_DIR', 'MORROW_MUTATION_CRASH_WASM_PATH', 'MORROW_MUTATION_DELIVERY_WASM_PATH', 'MORROW_FILE_CREATE_FAULT', 'MORROW_TEST_CRASH_AT')
$previous = @{}
foreach ($name in $variables) {
  $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
Push-Location $repo
$evidenceDirectory = $null
try {
  foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
  New-Item -ItemType Directory -Force -Path $Evidence | Out-Null
  $evidenceDirectory = (Resolve-Path -LiteralPath $Evidence).Path
  $runStarted = [DateTime]::UtcNow.ToString('o')
  # A failed rerun must not leave an earlier PASS summary as current evidence.
  [ordered]@{ status = 'running'; started_utc = $runStarted } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidenceDirectory 'summary.json') -Encoding utf8
  Checked $Python @('tool/sync_plugin_sdk_contracts.py', '--check')
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked $Python @('tool/verify_plugin_sdk_baseline.py')
  & ./tool/build_plugin_mutation_wasm.ps1 -Sysroot $Sysroot -Output $Output
  $directory = (Resolve-Path -LiteralPath $Output).Path
  $normal = TestBinary 'packages'
  $fault = TestBinary 'fault-injection'
  $normalHash = (Get-FileHash -LiteralPath $normal -Algorithm SHA256).Hash.ToLowerInvariant()
  $faultHash = (Get-FileHash -LiteralPath $fault -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($normal -eq $fault -or $normalHash -eq $faultHash) { throw 'Normal and fault test hosts must be distinct.' }

  # The same environment switches must have no effect in a normal host.
  $env:MORROW_MUTATION_SDK_WASM_DIR = $directory
  $env:MORROW_FILE_CREATE_FAULT = 'after-write-chunk'
  $env:MORROW_TEST_CRASH_AT = 'file-content-after-commit'
  $normalTests = @('c', 'cpp', 'rust') | ForEach-Object { "mutation_sdk_wasm::$($_)_mutation_sdk_uses_original_approved_owner" }
  # These SDK tests are explicitly ignored in the ordinary test suite.
  $listed = & $normal uses_original_approved_owner --list --ignored
  if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate normal host SDK controls.' }
  $actual = @($listed | Where-Object { $_ -match ': test$' } | Sort-Object)
  $expected = @($normalTests | ForEach-Object { "${_}: test" } | Sort-Object)
  if ($actual.Count -ne 3 -or (Compare-Object $expected $actual)) { throw 'Expected three normal SDK control tests.' }
  & $normal uses_original_approved_owner --ignored --test-threads=1 --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $evidenceDirectory 'normal-control.log')
  if ($LASTEXITCODE -ne 0) { throw 'Fault switches affected normal host controls.' }
  [Environment]::SetEnvironmentVariable('MORROW_FILE_CREATE_FAULT', $null, 'Process')
  [Environment]::SetEnvironmentVariable('MORROW_TEST_CRASH_AT', $null, 'Process')

  $deliveryTest = 'mutation_guest_delivery::multichunk_guest_recovers_lost_stage_commit_and_execute_receipts_without_replaying_effect'
  $crashTest = 'mutation_guest_crash::guest_create_real_process_crash_matrix_preserves_original_history'
  $results = @()
  foreach ($language in @('rust', 'c', 'cpp')) {
    $module = Join-Path $directory "${language}_mutation.wasm"
    $env:MORROW_MUTATION_DELIVERY_WASM_PATH = $module
    $env:MORROW_MUTATION_CRASH_WASM_PATH = $module
    RunTests $normal $deliveryTest @($deliveryTest) (Join-Path $evidenceDirectory "${language}-delivery.log")
    $crashLog = Join-Path $evidenceDirectory "${language}-crash.log"
    RunTests $fault $crashTest @($crashTest) $crashLog
    $boundaries = @('file-content-after-bytes', 'file-content-after-receipt', 'file-content-before-commit', 'file-content-after-commit', 'after-claim', 'after-temp', 'after-write-chunk', 'after-write', 'after-flush', 'after-publish', 'after-effect', 'after-observe')
    $expectedCases = @($boundaries | ForEach-Object { "PASS mutation crash boundary: $_" } | Sort-Object)
    $actualCases = @(Get-Content -LiteralPath $crashLog | Where-Object { $_ -match '^PASS mutation crash boundary: ' } | Sort-Object)
    if ($actualCases.Count -ne 12 -or (Compare-Object $expectedCases $actualCases)) { throw "Incomplete crash matrix for $language." }
    $results += [ordered]@{ language = $language; wasm_sha256 = (Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant(); delivery = 'passed'; crash_boundaries_passed = 12 }
  }
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked $Python @('tool/verify_plugin_sdk_baseline.py')
  [ordered]@{
    status = 'passed'
    started_utc = $runStarted
    scope = 'Windows Release original-owner multichunk guest delivery and process-exit recovery; not power-loss or full-product qualification'
    normal_host_sha256 = $normalHash
    fault_host_sha256 = $faultHash
    normal_control_tests = 3
    guests = $results
  } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidenceDirectory 'summary.json') -Encoding utf8
  Write-Output 'PASS: three SDK guests, multichunk lost receipts and real child-process exits; normal host fault switches inert.'
} catch {
  if ($evidenceDirectory) {
    [ordered]@{ status = 'failed'; started_utc = $runStarted; error = $_.Exception.Message } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidenceDirectory 'summary.json') -Encoding utf8
  }
  throw
} finally {
  foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
  Pop-Location
}
