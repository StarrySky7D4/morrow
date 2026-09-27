param(
  [string]$Sysroot = 'build/tools/wasi-34/wasi-sysroot-34.0',
  [string]$Output = 'build/mutation-sdk-wasm',
  [string]$Evidence = 'build/workbench-mutation-guest',
  [string]$Python = 'python',
  [switch]$PrivateWire
)
$ErrorActionPreference = 'Stop'
if ($PrivateWire -and -not $PSBoundParameters.ContainsKey('Evidence')) {
  $Evidence = 'build/workbench-guest-private-wire'
}
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Workbench mutation guest qualification currently requires Windows.'
}
$repo = Split-Path $PSScriptRoot -Parent
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Qualification failed: $Program (exit $LASTEXITCODE)" }
}
function HostBinary {
  $messages = & cargo test --release --locked --offline --manifest-path workbench_host/Cargo.toml --lib --no-run --message-format=json
  if ($LASTEXITCODE -ne 0) { throw 'Cannot build Workbench test host.' }
  $executables = @($messages | ForEach-Object {
    $entry = $_ | ConvertFrom-Json
    if ($entry.reason -eq 'compiler-artifact' -and $entry.target.name -eq 'morrow_workbench_host' -and $entry.profile.test -and $entry.executable) {
      $entry.executable
    }
  })
  if ($executables.Count -ne 1) { throw "Expected one Workbench test host, found $($executables.Count)." }
  return (Resolve-Path -LiteralPath $executables[0]).Path
}

$variables = @('MORROW_WORKBENCH_MUTATION_WASM_PATH', 'MORROW_FILE_CREATE_FAULT', 'MORROW_TEST_CRASH_AT', 'MORROW_GUEST_MUTATION_WIRE_FIXTURES')
$previous = @{}
foreach ($name in $variables) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$evidenceDirectory = $null
Push-Location $repo
try {
  foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
  New-Item -ItemType Directory -Force -Path $Evidence | Out-Null
  $evidenceDirectory = (Resolve-Path -LiteralPath $Evidence).Path
  $summaryPath = Join-Path $evidenceDirectory 'summary.json'
  $started = [DateTime]::UtcNow.ToString('o')
  [ordered]@{ status = 'running'; started_utc = $started } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
  Checked $Python @('tool/sync_plugin_sdk_contracts.py', '--check')
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked $Python @('tool/verify_plugin_sdk_baseline.py')
  & ./tool/build_plugin_mutation_wasm.ps1 -Sysroot $Sysroot -Output $Output
  $directory = (Resolve-Path -LiteralPath $Output).Path
  $binary = HostBinary
  $hostHash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
  $testName = 'io_tasks::mutation::guest::tests::guest_sdk_original_owner_create_delete'
  $scope = 'Windows Release Workbench Rust task API with three real SDK guests; not Flutter/private-wire/UI or cross-platform qualification'
  if ($PrivateWire) {
    $testName = 'io_tasks::mutation::guest::tests::guest_wire_tests::private_guest_wire_real_sdk_original_owner_create_delete'
    $scope = 'Windows Release private guest wire with three real SDK guests and original owner; not Flutter UI or cross-platform qualification'
  }
  $listed = & $binary $testName --exact --list
  if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate Workbench guest test.' }
  $inventory = @($listed | Where-Object { $_ -match ': test$' })
  if ($inventory.Count -ne 1 -or $inventory[0] -ne "${testName}: test") {
    throw "Unexpected test inventory: $($inventory -join ', ')"
  }
  $results = @()
  foreach ($language in @('rust', 'c', 'cpp')) {
    $module = (Resolve-Path -LiteralPath (Join-Path $directory "${language}_mutation.wasm")).Path
    $moduleHash = (Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant()
    $env:MORROW_WORKBENCH_MUTATION_WASM_PATH = $module
    if ($PrivateWire) {
      $env:MORROW_GUEST_MUTATION_WIRE_FIXTURES = Join-Path $evidenceDirectory "${language}-wire"
    }
    $log = Join-Path $evidenceDirectory "${language}-workbench.log"
    & $binary $testName --exact --test-threads=1 --nocapture 2>&1 | Tee-Object -FilePath $log
    if ($LASTEXITCODE -ne 0) { throw "Workbench guest failed for $language; see $log." }
    $logText = Get-Content -LiteralPath $log -Raw
    if (-not $logText.Contains('test result: ok. 1 passed; 0 failed; 0 ignored;')) {
      throw "Workbench guest did not execute exactly one passing test for $language."
    }
    $loadedModules = @([regex]::Matches($logText, 'WORKBENCH_GUEST_SHA256=([0-9a-f]{64})') | ForEach-Object { $_.Groups[1].Value })
    if ($loadedModules.Count -eq 0 -or @($loadedModules | Where-Object { $_ -ne $moduleHash }).Count -ne 0) {
      throw "Workbench test did not confirm loading the expected $language artifact."
    }
    if ((Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant() -ne $moduleHash) {
      throw "Guest artifact changed during qualification: $module"
    }
    $results += [ordered]@{ language = $language; wasm_sha256 = $moduleHash; test = $testName; result = 'passed' }
  }
  if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hostHash) {
    throw 'Workbench test host changed during qualification.'
  }
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked $Python @('tool/verify_plugin_sdk_baseline.py')
  [ordered]@{
    status = 'passed'
    started_utc = $started
    scope = $scope
    host_sha256 = $hostHash
    guests = $results
  } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $summaryPath -Encoding utf8
  Write-Output "PASS: $scope"
} catch {
  if ($evidenceDirectory) {
    [ordered]@{ status = 'failed'; started_utc = $started; error = $_.Exception.Message } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidenceDirectory 'summary.json') -Encoding utf8
  }
  throw
} finally {
  foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
  Pop-Location
}
