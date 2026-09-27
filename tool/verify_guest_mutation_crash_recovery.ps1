#requires -Version 7.0
param(
  [string]$Sysroot = '../tools/wasi-34/wasi-sysroot-34.0',
  [string]$Evidence = 'build/guest-mutation-crash-recovery',
  [string]$BuiltinPackage = 'build/workbench-host/bundle/workbench.morrowplugin',
  [string]$Flutter = 'flutter',
  [switch]$MaxContent
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Real guest mutation crash recovery qualification requires Windows.'
}
if ($MaxContent -and -not $PSBoundParameters.ContainsKey('Evidence')) {
  $Evidence = 'build/guest-mutation-max-crash-recovery'
}
$repo = Split-Path $PSScriptRoot -Parent
$variables = @(
  'MORROW_WORKBENCH_HOST', 'MORROW_MUTATION_CRASH_HOST', 'MORROW_WORKBENCH_PACKAGE',
  'MORROW_GUEST_MUTATION_PACKAGE', 'MORROW_GUEST_MUTATION_LANGUAGE',
  'MORROW_GUEST_MUTATION_WASM_SHA256', 'MORROW_GUEST_MUTATION_PACKAGE_SHA256',
  'MORROW_MUTATION_CRASH_KIND', 'MORROW_MUTATION_CRASH_POINT',
  'MORROW_MUTATION_EXPECT_CRASH', 'MORROW_FILE_CREATE_FAULT',
  'MORROW_FILE_DELETE_FAULT', 'MORROW_TEST_CRASH_AT',
  'MORROW_WORKBENCH_MUTATION_WASM_PATH',
  'MORROW_GUEST_MUTATION_CONTENT_BYTES'
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
  if ($MaxContent) {
    $env:MORROW_GUEST_MUTATION_CONTENT_BYTES = '16777216'
  }
  $evidenceRoot = [IO.Path]::GetFullPath((Join-Path $repo $Evidence))
  $builtin = (Resolve-Path -LiteralPath $BuiltinPackage).Path
  $builtinBefore = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
  $normalPath = Join-Path $repo 'build/workbench-host/release/morrow-workbench-host.exe'
  $packagerPath = Join-Path $repo 'build/workbench-host/release/examples/package_guest_mutation_fixture.exe'
  $faultTarget = Join-Path $evidenceRoot 'fault-target'
  $faultPath = Join-Path $faultTarget 'release/morrow-workbench-host.exe'
  $normalBefore = if (Test-Path -LiteralPath $normalPath) {
    (Get-FileHash -LiteralPath $normalPath -Algorithm SHA256).Hash.ToLowerInvariant()
  } else { $null }
  $packagerBefore = if (Test-Path -LiteralPath $packagerPath) {
    (Get-FileHash -LiteralPath $packagerPath -Algorithm SHA256).Hash.ToLowerInvariant()
  } else { $null }
  $faultBefore = if (Test-Path -LiteralPath $faultPath) {
    (Get-FileHash -LiteralPath $faultPath -Algorithm SHA256).Hash.ToLowerInvariant()
  } else { $null }
  New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
  $summary = Join-Path $evidenceRoot 'summary.json'
  [ordered]@{ status = 'running'; started_utc = $started } |
    ConvertTo-Json | Set-Content -LiteralPath $summary -Encoding utf8
  $run = Join-Path $evidenceRoot ('run-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
  $modules = Join-Path $run 'modules'
  $packages = Join-Path $run 'packages'
  $logs = Join-Path $run 'logs'
  New-Item -ItemType Directory -Path $modules, $packages, $logs | Out-Null
  $sourceFiles = @(
    'core/src/file_mutation.rs',
    'core/src/store/file_content.rs',
    'core/src/store/file_content_receipt.rs',
    'plugin_runtime/src/file_target/creation.rs',
    'plugin_runtime/src/file_target/native_windows.rs',
    'plugin_runtime/src/io_binding.rs',
    'plugin_runtime/src/io_jobs.rs',
    'plugin_runtime/src/io_jobs/mutation_commands.rs',
    'plugin_runtime/src/shared_objects.rs',
    'workbench_host/src/mutation_tasks.rs',
    'lib/plugins/guest_mutation_codec_native.dart',
    'lib/plugins/guest_mutation_execution_manager.dart',
    'lib/plugins/guest_mutation_execution_session.dart',
    'lib/plugins/guest_mutation_models.dart',
    'lib/plugins/guest_mutation_native.dart',
    'lib/plugins/host_request.dart',
    'lib/plugins/mutation_execution_manager.dart',
    'lib/plugins/mutation_execution_session.dart',
    'lib/plugins/mutation_recovery_manager.dart',
    'lib/plugins/mutation_recovery_session.dart',
    'lib/plugins/mutation_recovery_view_state.dart',
    'lib/plugins/mutation_task_codec_native.dart',
    'lib/plugins/mutation_task_models.dart',
    'lib/plugins/mutation_workflow.dart',
    'lib/plugins/workbench_native.dart',
    'test/guest_mutation_crash_support.dart',
    'test/guest_mutation_recovery_crash_native_test.dart',
    'test/guest_mutation_recovery_crash_widget_native_test.dart',
    'tool/verify_guest_mutation_crash_recovery.ps1'
  )
  $sources = @(foreach ($file in $sourceFiles) {
    [ordered]@{
      path = $file
      sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  })

  & ./tool/build_plugin_mutation_wasm.ps1 -Sysroot $Sysroot -Output $modules *> (Join-Path $run 'sdk-build.log')
  if (-not $?) { throw 'Three real SDK mutation Wasm modules did not build.' }
  & cargo build --locked --offline --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --bin morrow-workbench-host --example package_guest_mutation_fixture *> (Join-Path $run 'normal-build.log')
  if ($LASTEXITCODE -ne 0) { throw 'Normal host or fixture packager did not build.' }
  & cargo build --locked --offline --release --features fault-injection --manifest-path workbench_host/Cargo.toml --target-dir $faultTarget --bin morrow-workbench-host *> (Join-Path $run 'fault-build.log')
  if ($LASTEXITCODE -ne 0) { throw 'Isolated fault-injection host did not build.' }
  $normalHost = (Resolve-Path -LiteralPath $normalPath).Path
  $faultHost = (Resolve-Path -LiteralPath $faultPath).Path
  $packager = (Resolve-Path -LiteralPath $packagerPath).Path
  if ($normalHost -eq $faultHost) { throw 'Normal and fault host paths are identical.' }
  $normalHash = (Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant()
  $faultHash = (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant()
  $packagerHash = (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($normalHash -eq $faultHash) { throw 'Normal and fault host binaries are identical.' }
  $env:MORROW_WORKBENCH_HOST = $normalHost
  $env:MORROW_WORKBENCH_PACKAGE = $builtin
  $cases = [System.Collections.Generic.List[object]]::new()
  $guests = @()

  function Invoke-GuestCrashCase([string]$Language, [string]$Suite, [string]$Kind, [string]$Point, [bool]$ExpectCrash) {
    $mode = if ($ExpectCrash) { 'crash' } else { 'normal' }
    $name = "$Language-$Suite-$Kind-$Point-$mode"
    $env:MORROW_MUTATION_CRASH_HOST = if ($ExpectCrash) { $faultHost } else { $normalHost }
    $env:MORROW_MUTATION_CRASH_KIND = $Kind
    $env:MORROW_MUTATION_CRASH_POINT = $Point
    $env:MORROW_MUTATION_EXPECT_CRASH = if ($ExpectCrash) { '1' } else { '0' }
    [Environment]::SetEnvironmentVariable('MORROW_FILE_CREATE_FAULT', $null, 'Process')
    [Environment]::SetEnvironmentVariable('MORROW_FILE_DELETE_FAULT', $null, 'Process')
    [Environment]::SetEnvironmentVariable("MORROW_FILE_$($Kind.ToUpperInvariant())_FAULT", $Point, 'Process')
    $testFile = if ($Suite -eq 'native') {
      'test/guest_mutation_recovery_crash_native_test.dart'
    } else {
      'test/guest_mutation_recovery_crash_widget_native_test.dart'
    }
    if (-not (Test-Path -LiteralPath $testFile)) { throw "Missing $Suite crash test: $testFile" }
    $log = Join-Path $logs "$name.log"
    & $Flutter test --no-pub --reporter expanded $testFile 2>&1 | Tee-Object -FilePath $log
    if ($LASTEXITCODE -ne 0) { throw "Real guest crash case $name failed; see $log." }
    $result = Get-Content -LiteralPath $log -Raw
    $marker = if ($Suite -eq 'native') {
      "GUEST_CRASH_RECOVERY_PASS=$Language`:$Kind`:$Point`:$($env:MORROW_MUTATION_EXPECT_CRASH)"
    } else {
      "GUEST_CRASH_WIDGET_PASS=$Kind`:$Point`:$mode"
    }
    if (-not $result.Contains($marker) -or
        ($Suite -eq 'native' -and -not $result.Contains("GUEST_CRASH_RECOVERY_PACKAGE_SHA256=$($env:MORROW_GUEST_MUTATION_PACKAGE_SHA256)")) -or
        -not ($result -match '\+1: All tests passed!') -or
        $result.Contains('Skip:')) {
      throw "Real guest crash case $name has missing pass marker, zero/ignored tests, or Skip; see $log."
    }
    if ($MaxContent) {
      $expectedContent = if ($Kind -eq 'create') { 16777216 } else { 3 }
      if ($Suite -eq 'native') {
        $expectedChunks = if ($Kind -eq 'create') { 274 } else { 0 }
        if (-not $result.Contains("GUEST_CRASH_CONTENT_BYTES=$expectedContent") -or
            -not $result.Contains("GUEST_CRASH_CHUNKS=$expectedChunks") -or
            -not ($result -match 'GUEST_CRASH_REVIEW_MS=\d+') -or
            -not ($result -match 'GUEST_CRASH_PREPARE_MS=\d+') -or
            -not ($result -match 'GUEST_CRASH_EXECUTE_MS=\d+')) {
          throw "Missing exact maximum-content size, chunks, or phase timing in $name; see $log."
        }
      } else {
        $expectedWidgetChunks = if ($Kind -eq 'create') { 274 } else { 0 }
        if (-not $result.Contains("GUEST_CRASH_WIDGET_CONTENT_BYTES=$expectedContent") -or
            -not $result.Contains("GUEST_CRASH_WIDGET_CHUNKS=$expectedWidgetChunks")) {
          throw "Missing exact maximum-content Widget size or chunks in $name; see $log."
        }
      }
    }
    if ((Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $normalHash -or
        (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $faultHash -or
        (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packagerHash -or
        (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant() -ne $builtinBefore -or
        (Get-FileHash -LiteralPath $env:MORROW_GUEST_MUTATION_PACKAGE -Algorithm SHA256).Hash.ToLowerInvariant() -ne $env:MORROW_GUEST_MUTATION_PACKAGE_SHA256) {
      throw "Host, packager, builtin, or guest package changed during $name."
    }
    $cases.Add([ordered]@{
      name = $name; language = $Language; suite = $Suite
      kind = $Kind; point = $Point; expected_crash = $ExpectCrash
      marker = $marker; status = 'passed'; log = $log
    })
  }

  $createFaultPoints = if ($MaxContent) {
    @('after-claim', 'after-temp', 'after-write-chunk', 'after-write',
      'after-flush', 'after-publish', 'after-effect', 'after-observe')
  } else {
    @('after-claim', 'after-effect', 'after-observe')
  }
  $deleteFaultPoints = @('after-claim', 'after-effect', 'after-observe')
  foreach ($language in @('rust', 'c', 'cpp')) {
    $module = (Resolve-Path -LiteralPath (Join-Path $modules "${language}_mutation.wasm")).Path
    $moduleHash = (Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant()
    $package = Join-Path $packages "${language}_mutation.morrowplugin"
    $packageLog = Join-Path $run "${language}-package.log"
    & $packager $language $module $package 2>&1 | Tee-Object -FilePath $packageLog
    if ($LASTEXITCODE -ne 0) { throw "Cannot package $language guest fixture." }
    $packageHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
    $packageText = Get-Content -LiteralPath $packageLog -Raw
    if (-not $packageText.Contains("WASM_SHA256=$moduleHash") -or
        -not $packageText.Contains("PACKAGE_SHA256=$packageHash") -or
        -not $packageText.Contains('MUTATION_MAX_JOB_BYTES=33554432') -or
        -not $packageText.Contains('MUTATION_MAX_BYTES=268435456')) {
      throw "Unverified SDK module/package digest or mutation budget for $language."
    }
    $env:MORROW_GUEST_MUTATION_PACKAGE = $package
    $env:MORROW_GUEST_MUTATION_LANGUAGE = $language
    $env:MORROW_GUEST_MUTATION_WASM_SHA256 = $moduleHash
    $env:MORROW_GUEST_MUTATION_PACKAGE_SHA256 = $packageHash
    foreach ($kind in @('create', 'delete')) {
      $points = if ($kind -eq 'create') { $createFaultPoints } else { $deleteFaultPoints }
      foreach ($point in $points) {
        Invoke-GuestCrashCase $language 'native' $kind $point $true
      }
    }
    foreach ($kind in @('create', 'delete')) {
      Invoke-GuestCrashCase $language 'native' $kind 'after-claim' $false
    }
    if ($language -eq 'rust') {
      foreach ($kind in @('create', 'delete')) {
        $points = if ($kind -eq 'create') { $createFaultPoints } else { $deleteFaultPoints }
        foreach ($point in $points) {
          Invoke-GuestCrashCase $language 'widget' $kind $point $true
        }
      }
      foreach ($kind in @('create', 'delete')) {
        Invoke-GuestCrashCase $language 'widget' $kind 'after-claim' $false
      }
    }
    if ((Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant() -ne $moduleHash -or
        (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packageHash) {
      throw "SDK module or package changed during $language matrix."
    }
    $guests += [ordered]@{
      language = $language; wasm_sha256 = $moduleHash; package_sha256 = $packageHash
    }
  }
  $expectedTotal = if ($MaxContent) { 52 } else { 32 }
  $expectedNative = if ($MaxContent) { 39 } else { 24 }
  $expectedWidget = if ($MaxContent) { 13 } else { 8 }
  if ($cases.Count -ne $expectedTotal -or
      @($cases | Where-Object suite -eq 'native').Count -ne $expectedNative -or
      @($cases | Where-Object suite -eq 'widget').Count -ne $expectedWidget) {
    throw "Incomplete guest crash matrix: $($cases.Count) cases."
  }
  foreach ($source in $sources) {
    if ((Get-FileHash -LiteralPath $source.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $source.sha256) {
      throw "Source changed during guest crash matrix: $($source.path)."
    }
  }
  if ((Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $normalHash -or
      (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $faultHash -or
      (Get-FileHash -LiteralPath $packager -Algorithm SHA256).Hash.ToLowerInvariant() -ne $packagerHash -or
      (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant() -ne $builtinBefore) {
    throw 'Pinned host, packager, or builtin package changed during crash qualification.'
  }
  [ordered]@{
    status = 'passed'; started_utc = $started
    scope = if ($MaxContent) {
      'Windows real SDK 16 MiB guest Create at eight file-effect process-exit points and Delete at three, normal-host controls, and Rust recovery Widget; Core content-transaction fault points and power loss are not covered'
    } else {
      'Windows real SDK guest process exits at three fault points for Create/Delete, normal host inert controls, and Rust recovery Widget; no power-loss claim'
    }
    profile = if ($MaxContent) { 'maximum-content-v1' } else { 'standard' }
    create_content_bytes = if ($MaxContent) { 16777216 } else { $null }
    create_chunk_bytes = if ($MaxContent) { 61440 } else { $null }
    create_chunks = if ($MaxContent) { 274 } else { $null }
    normal_host_sha256_before_build = $normalBefore; normal_host_sha256 = $normalHash
    fault_host_sha256_before_build = $faultBefore; fault_host_sha256 = $faultHash
    packager_sha256_before_build = $packagerBefore; packager_sha256 = $packagerHash
    builtin_package_sha256_before = $builtinBefore; builtin_package_sha256_after = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
    run_directory = $run; source_sha256 = $sources; guests = $guests; cases = $cases
  } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $summary -Encoding utf8
  if ($MaxContent) {
    Write-Output 'PASS: 33 maximum-content SDK crash cases, 6 normal-host inert controls, 11 Rust Widget crash cases, and 2 Rust Widget controls.'
  } else {
    Write-Output 'PASS: 18 guest SDK crash cases, 6 normal-host inert controls, 6 Rust Widget crash cases, and 2 Rust Widget controls.'
  }
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
