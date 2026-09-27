#requires -Version 7.0
param(
  [string]$Sysroot = '../tools/wasi-34/wasi-sysroot-34.0',
  [string]$Evidence = 'build/guest-mutation-recovery-ui',
  [string]$BuiltinPackage = 'build/workbench-host/bundle/workbench.morrowplugin',
  [string]$Flutter = 'flutter'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Real guest mutation recovery qualification currently requires Windows.'
}
$repo = Split-Path $PSScriptRoot -Parent
$variables = @(
  'MORROW_WORKBENCH_HOST', 'MORROW_WORKBENCH_PACKAGE',
  'MORROW_GUEST_MUTATION_PACKAGE', 'MORROW_GUEST_MUTATION_LANGUAGE',
  'MORROW_GUEST_MUTATION_WASM_SHA256', 'MORROW_GUEST_MUTATION_PACKAGE_SHA256',
  'MORROW_WORKBENCH_MUTATION_WASM_PATH', 'MORROW_FILE_CREATE_FAULT',
  'MORROW_FILE_DELETE_FAULT', 'MORROW_TEST_CRASH_AT'
)
$previous = @{}
foreach ($name in $variables) {
  $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$evidenceRoot = $null
$started = [DateTime]::UtcNow.ToString('o')
Push-Location -LiteralPath $repo
try {
  foreach ($name in $variables) {
    [Environment]::SetEnvironmentVariable($name, $null, 'Process')
  }
  $builtin = (Resolve-Path -LiteralPath $BuiltinPackage).Path
  $builtinBefore = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
  $hostPath = Join-Path $repo 'build/workbench-host/release/morrow-workbench-host.exe'
  $packagerPath = Join-Path $repo 'build/workbench-host/release/examples/package_guest_mutation_fixture.exe'
  $hostBefore = if (Test-Path -LiteralPath $hostPath) {
    (Get-FileHash -LiteralPath $hostPath -Algorithm SHA256).Hash.ToLowerInvariant()
  } else { $null }
  $packagerBefore = if (Test-Path -LiteralPath $packagerPath) {
    (Get-FileHash -LiteralPath $packagerPath -Algorithm SHA256).Hash.ToLowerInvariant()
  } else { $null }
  $evidenceRoot = [IO.Path]::GetFullPath((Join-Path $repo $Evidence))
  New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
  $summary = Join-Path $evidenceRoot 'summary.json'
  [ordered]@{ status = 'running'; started_utc = $started } |
    ConvertTo-Json | Set-Content -LiteralPath $summary -Encoding utf8
  $run = Join-Path $evidenceRoot ('run-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
  $modules = Join-Path $run 'modules'
  $packages = Join-Path $run 'packages'
  New-Item -ItemType Directory -Path $modules, $packages | Out-Null

  & ./tool/build_plugin_mutation_wasm.ps1 -Sysroot $Sysroot -Output $modules *> (Join-Path $run 'sdk-build.log')
  if (-not $?) { throw 'Three real SDK mutation Wasm modules did not build.' }
  & cargo build --locked --offline --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --bin morrow-workbench-host --example package_guest_mutation_fixture *> (Join-Path $run 'host-build.log')
  if ($LASTEXITCODE -ne 0) { throw 'Current host and mutation fixture packager did not build.' }
  $hostBinary = (Resolve-Path -LiteralPath $hostPath).Path
  $packager = (Resolve-Path -LiteralPath $packagerPath).Path
  $hostAfterBuild = (Get-FileHash -LiteralPath $hostBinary -Algorithm SHA256).Hash.ToLowerInvariant()
  $packagerAfterBuild = (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant()
  $env:MORROW_WORKBENCH_HOST = $hostBinary
  $env:MORROW_WORKBENCH_PACKAGE = $builtin
  $rows = @()
  foreach ($language in @('rust', 'c', 'cpp')) {
    $module = (Resolve-Path -LiteralPath (Join-Path $modules "${language}_mutation.wasm")).Path
    $moduleHash = (Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant()
    $package = Join-Path $packages "${language}_mutation.morrowplugin"
    $packageLog = Join-Path $run "${language}-package.log"
    & $packager $language $module $package 2>&1 | Tee-Object -FilePath $packageLog
    if ($LASTEXITCODE -ne 0) { throw "Cannot package $language guest mutation fixture." }
    $packageHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
    $packageText = Get-Content -LiteralPath $packageLog -Raw
    if (-not $packageText.Contains("WASM_SHA256=$moduleHash") -or
        -not $packageText.Contains("PACKAGE_SHA256=$packageHash") -or
        -not $packageText.Contains('MUTATION_MAX_JOB_BYTES=33554432') -or
        -not $packageText.Contains('MUTATION_MAX_BYTES=268435456')) {
      throw "Unverified real module/package digest or budget for $language."
    }
    $env:MORROW_GUEST_MUTATION_PACKAGE = $package
    $env:MORROW_GUEST_MUTATION_LANGUAGE = $language
    $env:MORROW_GUEST_MUTATION_WASM_SHA256 = $moduleHash
    $env:MORROW_GUEST_MUTATION_PACKAGE_SHA256 = $packageHash
    $log = Join-Path $run "${language}-restart-recovery.log"
    & $Flutter test --no-pub --reporter expanded test/guest_mutation_recovery_real_native_test.dart 2>&1 | Tee-Object -FilePath $log
    if ($LASTEXITCODE -ne 0) { throw "Real $language restart recovery failed; see $log." }
    $result = Get-Content -LiteralPath $log -Raw
    if (-not $result.Contains("GUEST_RECOVERY_PASS=$language") -or
        -not $result.Contains("GUEST_RECOVERY_PACKAGE_SHA256=$packageHash") -or
        -not ($result -match 'GUEST_RECOVERY_ORIGINAL_SHA256=[0-9a-f]{64}') -or
        -not $result.Contains('All tests passed!') -or
        $result.Contains('Skip:')) {
      throw "Real $language restart recovery did not execute a passing case."
    }
    if ((Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant() -ne $moduleHash -or
        (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packageHash) {
      throw "Guest fixture changed during $language recovery qualification."
    }
    $rows += [ordered]@{
      language = $language; wasm_sha256 = $moduleHash
      package_sha256 = $packageHash; normal_restart_recovery = 'passed'
    }
  }
  if ((Get-FileHash -LiteralPath $hostBinary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hostAfterBuild -or
      (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packagerAfterBuild -or
      (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant() -ne $builtinBefore) {
    throw 'Host, packager, or existing builtin package changed during qualification.'
  }
  [ordered]@{
    status = 'passed'; started_utc = $started
    scope = 'Windows real SDK guest nonempty Create, normal new-process restart, current-approval discovery and read-only reconciliation; no process-crash or power-loss claim'
    host_sha256_before_build = $hostBefore; host_sha256_after_build = $hostAfterBuild
    packager_sha256_before_build = $packagerBefore; packager_sha256_after_build = $packagerAfterBuild
    builtin_package_sha256_before = $builtinBefore; builtin_package_sha256_after = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
    run_directory = $run; guests = $rows
  } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $summary -Encoding utf8
  Write-Output 'PASS: three real SDK guest mutation normal-restart read-only recovery cases.'
} catch {
  if ($evidenceRoot) {
    [ordered]@{ status = 'failed'; started_utc = $started; error = $_.Exception.Message } |
      ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidenceRoot 'summary.json') -Encoding utf8
  }
  throw
} finally {
  foreach ($name in $variables) {
    [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
  }
  Pop-Location
}
