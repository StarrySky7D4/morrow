#requires -Version 7.0
param(
  [string]$Sysroot = '../tools/wasi-34/wasi-sysroot-34.0',
  [string]$Evidence = 'build/guest-mutation-ui',
  [string]$BuiltinPackage = 'build/workbench-host/bundle/workbench.morrowplugin',
  [string]$Flutter = 'flutter'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Real guest mutation UI qualification currently requires Windows.'
}
$repo = Split-Path $PSScriptRoot -Parent
$vars = @(
  'MORROW_WORKBENCH_HOST', 'MORROW_WORKBENCH_PACKAGE',
  'MORROW_GUEST_MUTATION_PACKAGE', 'MORROW_GUEST_MUTATION_LANGUAGE',
  'MORROW_GUEST_MUTATION_WASM_SHA256', 'MORROW_WORKBENCH_MUTATION_WASM_PATH',
  'MORROW_FILE_CREATE_FAULT', 'MORROW_FILE_DELETE_FAULT', 'MORROW_TEST_CRASH_AT'
)
$previous = @{}
foreach ($name in $vars) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$evidenceRoot = $null
$started = [DateTime]::UtcNow.ToString('o')
Push-Location -LiteralPath $repo
try {
  foreach ($name in $vars) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
  $builtin = (Resolve-Path -LiteralPath $BuiltinPackage).Path
  $builtinHashBefore = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
  $hostPath = Join-Path $repo 'build/workbench-host/release/morrow-workbench-host.exe'
  $packagerPath = Join-Path $repo 'build/workbench-host/release/examples/package_guest_mutation_fixture.exe'
  $hostHashBefore = if (Test-Path -LiteralPath $hostPath) { (Get-FileHash -LiteralPath $hostPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
  $packagerHashBefore = if (Test-Path -LiteralPath $packagerPath) { (Get-FileHash -LiteralPath $packagerPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
  $evidenceRoot = [IO.Path]::GetFullPath((Join-Path $repo $Evidence))
  New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
  $summaryPath = Join-Path $evidenceRoot 'summary.json'
  @{ status = 'running'; started_utc = $started } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
  $run = Join-Path $evidenceRoot ('run-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
  $modules = Join-Path $run 'modules'
  $packages = Join-Path $run 'packages'
  New-Item -ItemType Directory -Path $modules, $packages | Out-Null

  & ./tool/build_plugin_mutation_wasm.ps1 -Sysroot $Sysroot -Output $modules *> (Join-Path $run 'sdk-build.log')
  if (-not $?) { throw 'Three real SDK mutation Wasm modules did not build.' }
  & cargo build --locked --offline --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --bin morrow-workbench-host --example package_guest_mutation_fixture *> (Join-Path $run 'host-build.log')
  if ($LASTEXITCODE -ne 0) { throw 'Current guest-wire host and fixture packager did not build.' }
  $hostBinary = (Resolve-Path -LiteralPath $hostPath).Path
  $packager = (Resolve-Path -LiteralPath 'build/workbench-host/release/examples/package_guest_mutation_fixture.exe').Path
  $hostHashAfterBuild = (Get-FileHash -LiteralPath $hostBinary -Algorithm SHA256).Hash.ToLowerInvariant()
  $packagerHashAfterBuild = (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant()
  $env:MORROW_WORKBENCH_HOST = $hostBinary
  $env:MORROW_WORKBENCH_PACKAGE = $builtin
  $rows = @()
  foreach ($language in @('rust', 'c', 'cpp')) {
    $module = (Resolve-Path -LiteralPath (Join-Path $modules "${language}_mutation.wasm")).Path
    $package = Join-Path $packages "${language}_mutation.morrowplugin"
    $moduleHash = (Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant()
    $packageLog = Join-Path $run "${language}-package.log"
    & $packager $language $module $package 2>&1 | Tee-Object -FilePath $packageLog
    if ($LASTEXITCODE -ne 0) { throw "Cannot package $language guest mutation fixture." }
    $packageText = Get-Content -LiteralPath $packageLog -Raw
    if (-not $packageText.Contains("WASM_SHA256=$moduleHash") -or
        -not $packageText.Contains('MUTATION_MAX_JOB_BYTES=33554432') -or
        -not $packageText.Contains('MUTATION_MAX_BYTES=268435456')) {
      throw "Unverified mutation budget or module hash in $language package."
    }
    $packageHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not $packageText.Contains("PACKAGE_SHA256=$packageHash")) {
      throw "Package SHA-256 mismatch for $language."
    }
    $env:MORROW_GUEST_MUTATION_PACKAGE = $package
    $env:MORROW_GUEST_MUTATION_LANGUAGE = $language
    $env:MORROW_GUEST_MUTATION_WASM_SHA256 = $moduleHash
    $log = Join-Path $run "${language}-session.log"
    & $Flutter test --no-pub --reporter expanded test/guest_mutation_execution_real_native_test.dart 2>&1 | Tee-Object -FilePath $log
    if ($LASTEXITCODE -ne 0) { throw "Real $language guest Session test failed; see $log." }
    $logText = Get-Content -LiteralPath $log -Raw
    if (-not $logText.Contains("GUEST_NATIVE_PASS=$language") -or
        -not $logText.Contains("GUEST_PACKAGE_SHA256=$packageHash") -or
        -not $logText.Contains('All tests passed!') -or
        $logText.Contains('Skip:')) {
      throw "Real $language guest Session did not execute a passing case."
    }
    if ((Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant() -ne $moduleHash -or
        (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packageHash) {
      throw "Guest fixture changed during $language qualification."
    }
    $rows += [ordered]@{ language = $language; wasm_sha256 = $moduleHash; package_sha256 = $packageHash; session = 'passed' }
    if ($language -eq 'rust') {
      $widgetLog = Join-Path $run 'rust-widget.log'
      & $Flutter test --no-pub --reporter expanded test/guest_mutation_execution_widget_native_test.dart 2>&1 | Tee-Object -FilePath $widgetLog
      if ($LASTEXITCODE -ne 0) { throw "Real Rust guest Widget test failed; see $widgetLog." }
      $widgetText = Get-Content -LiteralPath $widgetLog -Raw
      if (-not $widgetText.Contains('GUEST_WIDGET_PASS=rust') -or
          -not $widgetText.Contains('All tests passed!') -or
          $widgetText.Contains('Skip:')) {
        throw 'Real Rust guest Widget did not execute a passing case.'
      }
      $rows[-1].widget = 'passed'
    }
  }
  if ((Get-FileHash -LiteralPath $hostBinary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hostHashAfterBuild -or
      (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packagerHashAfterBuild -or
      (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant() -ne $builtinHashBefore) {
    throw 'Host, packager, or existing builtin package changed during qualification.'
  }
  [ordered]@{
    status = 'passed'; started_utc = $started
    scope = 'Windows real Flutter native guest mutation Session for Rust/C/C++ and real Rust Widget two-stage approval; isolated temporary libraries only'
    host_sha256_before_build = $hostHashBefore; host_sha256_after_build = $hostHashAfterBuild
    packager_sha256_before_build = $packagerHashBefore; packager_sha256_after_build = $packagerHashAfterBuild
    builtin_package_sha256_before = $builtinHashBefore; builtin_package_sha256_after = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
    run_directory = $run; guests = $rows
  } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $summaryPath -Encoding utf8
  Write-Output 'PASS: three real SDK guest mutation Sessions and Rust two-stage approval Widget.'
} catch {
  if ($evidenceRoot) {
    [ordered]@{ status = 'failed'; started_utc = $started; error = $_.Exception.Message } |
      ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidenceRoot 'summary.json') -Encoding utf8
  }
  throw
} finally {
  foreach ($name in $vars) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
  Pop-Location
}
