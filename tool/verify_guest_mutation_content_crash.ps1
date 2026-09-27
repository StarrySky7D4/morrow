#requires -Version 7.0
param(
  [string]$Evidence = 'build/guest-mutation-content-crash-recovery',
  [string]$PriorSummary = 'build/guest-mutation-max-crash-recovery/summary.json',
  [string]$BuiltinPackage = 'build/workbench-host/bundle/workbench.morrowplugin',
  [string]$Flutter = 'flutter'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'Real Core content transaction crash qualification requires Windows.'
}
$repo = Split-Path $PSScriptRoot -Parent
$variables = @(
  'MORROW_WORKBENCH_HOST', 'MORROW_MUTATION_CRASH_HOST', 'MORROW_WORKBENCH_PACKAGE',
  'MORROW_GUEST_MUTATION_PACKAGE', 'MORROW_GUEST_MUTATION_LANGUAGE',
  'MORROW_GUEST_MUTATION_WASM_SHA256', 'MORROW_GUEST_MUTATION_PACKAGE_SHA256',
  'MORROW_GUEST_MUTATION_STORE_VERIFIER', 'MORROW_GUEST_MUTATION_CONTENT_BYTES',
  'MORROW_MUTATION_CRASH_KIND', 'MORROW_MUTATION_CRASH_POINT',
  'MORROW_MUTATION_EXPECT_CRASH', 'MORROW_TEST_CRASH_AT',
  'MORROW_FILE_CREATE_FAULT', 'MORROW_FILE_DELETE_FAULT',
  'MORROW_WORKBENCH_MUTATION_WASM_PATH'
)
$previous = @{}
foreach ($name in $variables) {
  $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$evidenceRoot = $null
$run = $null
$cases = [System.Collections.Generic.List[object]]::new()
$started = [DateTime]::UtcNow.ToString('o')
Push-Location -LiteralPath $repo
try {
  foreach ($name in $variables) {
    [Environment]::SetEnvironmentVariable($name, $null, 'Process')
  }
  $evidenceRoot = [IO.Path]::GetFullPath((Join-Path $repo $Evidence))
  New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
  $summaryPath = Join-Path $evidenceRoot 'summary.json'
  [ordered]@{ status = 'running'; started_utc = $started } |
    ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
  $priorPath = (Resolve-Path -LiteralPath $PriorSummary).Path
  $priorHash = (Get-FileHash -LiteralPath $priorPath -Algorithm SHA256).Hash.ToLowerInvariant()
  $prior = Get-Content -LiteralPath $priorPath -Raw | ConvertFrom-Json
  if ($prior.status -ne 'passed' -or $prior.profile -ne 'maximum-content-v1' -or
      $prior.create_content_bytes -ne 16777216 -or $prior.create_chunks -ne 274 -or
      @($prior.cases).Count -ne 52 -or @($prior.guests).Count -ne 3) {
    throw 'Prior 52-case maximum-content qualification is missing or incomplete.'
  }
  $priorRun = (Resolve-Path -LiteralPath $prior.run_directory).Path
  $normalHost = (Resolve-Path -LiteralPath 'build/workbench-host/release/morrow-workbench-host.exe').Path
  $faultHost = (Resolve-Path -LiteralPath 'build/guest-mutation-max-crash-recovery/fault-target/release/morrow-workbench-host.exe').Path
  $builtin = (Resolve-Path -LiteralPath $BuiltinPackage).Path
  $normalHash = (Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant()
  $faultHash = (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant()
  $builtinHash = (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($normalHost -eq $faultHost -or $normalHash -eq $faultHash -or
      $normalHash -ne $prior.normal_host_sha256 -or
      $faultHash -ne $prior.fault_host_sha256 -or
      $builtinHash -ne $prior.builtin_package_sha256_after) {
    throw 'Ordinary host, isolated fault host, or builtin package differs from the qualified pinned artifacts.'
  }
  $run = Join-Path $evidenceRoot ('run-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
  $logs = Join-Path $run 'logs'
  New-Item -ItemType Directory -Path $run, $logs | Out-Null
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
    'workbench_host/examples/verify_guest_mutation_store.rs',
    'lib/plugins/guest_mutation_codec_native.dart',
    'lib/plugins/guest_mutation_execution_manager.dart',
    'lib/plugins/guest_mutation_execution_session.dart',
    'lib/plugins/guest_mutation_models.dart',
    'lib/plugins/guest_mutation_native.dart',
    'lib/plugins/host_request.dart',
    'lib/plugins/io_task_models.dart',
    'lib/plugins/mutation_workflow.dart',
    'lib/plugins/mutation_recovery_manager.dart',
    'lib/plugins/mutation_recovery_session.dart',
    'lib/plugins/mutation_recovery_view_state.dart',
    'lib/plugins/mutation_task_codec_native.dart',
    'lib/plugins/mutation_task_models.dart',
    'lib/plugins/plugin_library.dart',
    'lib/plugins/session_view_state.dart',
    'lib/plugins/workbench_native.dart',
    'test/external_plugin_native_test.dart',
    'test/guest_mutation_crash_support.dart',
    'test/guest_mutation_content_crash_native_test.dart',
    'test/guest_mutation_content_crash_widget_native_test.dart',
    'tool/verify_guest_mutation_content_crash.ps1'
  )
  $sources = @(foreach ($file in $sourceFiles) {
    [ordered]@{
      path = $file
      sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  })

  # Build only the read-only Store verifier. Neither qualified host binary is rebuilt.
  & cargo build --locked --offline --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --example verify_guest_mutation_store *> (Join-Path $run 'store-verifier-build.log')
  if ($LASTEXITCODE -ne 0) { throw 'Read-only Store verification example did not build.' }
  $storeVerifier = (Resolve-Path -LiteralPath 'build/workbench-host/release/examples/verify_guest_mutation_store.exe').Path
  $storeVerifierHash = (Get-FileHash -LiteralPath $storeVerifier -Algorithm SHA256).Hash.ToLowerInvariant()
  if ((Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $normalHash -or
      (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $faultHash) {
    throw 'Building the Store verifier changed a qualified host binary.'
  }
  $env:MORROW_WORKBENCH_HOST = $normalHost
  $env:MORROW_WORKBENCH_PACKAGE = $builtin
  $env:MORROW_GUEST_MUTATION_STORE_VERIFIER = $storeVerifier
  $env:MORROW_GUEST_MUTATION_CONTENT_BYTES = '16777216'
  $artifactPins = @()
  foreach ($language in @('rust', 'c', 'cpp')) {
    $guest = @($prior.guests | Where-Object language -eq $language)
    if ($guest.Count -ne 1) { throw "Missing unique $language guest digest in prior qualification." }
    $module = (Resolve-Path -LiteralPath (Join-Path $priorRun "modules/${language}_mutation.wasm")).Path
    $package = (Resolve-Path -LiteralPath (Join-Path $priorRun "packages/${language}_mutation.morrowplugin")).Path
    $moduleHash = (Get-FileHash -LiteralPath $module -Algorithm SHA256).Hash.ToLowerInvariant()
    $packageHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($moduleHash -ne $guest[0].wasm_sha256 -or $packageHash -ne $guest[0].package_sha256) {
      throw "Prior $language SDK module or package digest changed."
    }
    $artifactPins += [ordered]@{
      language = $language; module = $module; module_sha256 = $moduleHash
      package = $package; package_sha256 = $packageHash
    }
  }

  function Invoke-ContentCase([string]$Language, [string]$Suite, [string]$Point, [bool]$ExpectCrash) {
    $mode = if ($ExpectCrash) { 'crash' } else { 'normal' }
    $name = "$Language-$Suite-$Point-$mode"
    $env:MORROW_MUTATION_CRASH_HOST = if ($ExpectCrash) { $faultHost } else { $normalHost }
    $env:MORROW_TEST_CRASH_AT = $Point
    $env:MORROW_MUTATION_EXPECT_CRASH = if ($ExpectCrash) { '1' } else { '0' }
    $testFile = if ($Suite -eq 'native') {
      'test/guest_mutation_content_crash_native_test.dart'
    } else {
      'test/guest_mutation_content_crash_widget_native_test.dart'
    }
    $log = Join-Path $logs "$name.log"
    & $Flutter test --no-pub --reporter expanded $testFile 2>&1 | Tee-Object -FilePath $log
    if ($LASTEXITCODE -ne 0) { throw "Core content transaction case $name failed; see $log." }
    $result = Get-Content -LiteralPath $log -Raw
    $marker = if ($Suite -eq 'native') {
      "GUEST_CONTENT_CRASH_PASS=$Language`:$Point`:$($env:MORROW_MUTATION_EXPECT_CRASH)"
    } else {
      "GUEST_CONTENT_CRASH_WIDGET_PASS=$Point`:$mode"
    }
    $committed = if (-not $ExpectCrash -or $Point -eq 'file-content-after-commit') { 1 } else { 0 }
    $storedBytes = if ($committed -eq 1) { 16777216 } else { 0 }
    $contentMarkers = if ($Suite -eq 'native') {
      @('GUEST_CRASH_CONTENT_BYTES=16777216', 'GUEST_CRASH_CHUNKS=274', 'GUEST_CONTENT_EXECUTE_CALLS=0')
    } else {
      @('GUEST_CRASH_CONTENT_BYTES=16777216', 'GUEST_CRASH_CHUNKS=274',
        'GUEST_CONTENT_CRASH_WIDGET_BYTES=16777216',
        'GUEST_CONTENT_CRASH_WIDGET_CHUNKS=274', 'GUEST_CONTENT_EXECUTE_CALLS=0')
    }
    foreach ($required in @($marker, "GUEST_CONTENT_STORE_PASS=$committed", "GUEST_CONTENT_STORE_BYTES=$storedBytes") + $contentMarkers) {
      if (-not [regex]::IsMatch($result, "(?m)^$([regex]::Escape($required))\r?$")) {
        throw "Missing exact-line $required in $name; see $log."
      }
    }
    if (-not [regex]::IsMatch($result, '(?m)^GUEST_CONTENT_BODY_SHA256=[0-9a-f]{64}\r?$')) {
      throw "Missing independently checked Store body digest in $name; see $log."
    }
    if ($result -notmatch '\+1: All tests passed!' -or $result.Contains('Skip:')) {
      throw "Zero/ignored test or Skip in $name; see $log."
    }
    if ((Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $normalHash -or
        (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $faultHash -or
        (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant() -ne $builtinHash -or
        (Get-FileHash -LiteralPath $storeVerifier -Algorithm SHA256).Hash.ToLowerInvariant() -ne $storeVerifierHash -or
        (Get-FileHash -LiteralPath $env:MORROW_GUEST_MUTATION_PACKAGE -Algorithm SHA256).Hash.ToLowerInvariant() -ne $env:MORROW_GUEST_MUTATION_PACKAGE_SHA256) {
      throw "Pinned host, builtin, Store verifier, or guest package changed during $name."
    }
    $cases.Add([ordered]@{
      name = $name; language = $Language; suite = $Suite
      point = $Point; expected_crash = $ExpectCrash
      expected_committed = $committed; marker = $marker; log = $log; status = 'passed'
    })
  }

  $points = @(
    'file-content-after-bytes', 'file-content-after-receipt',
    'file-content-before-commit', 'file-content-after-commit'
  )
  foreach ($pin in $artifactPins) {
    $env:MORROW_GUEST_MUTATION_LANGUAGE = $pin.language
    $env:MORROW_GUEST_MUTATION_PACKAGE = $pin.package
    $env:MORROW_GUEST_MUTATION_WASM_SHA256 = $pin.module_sha256
    $env:MORROW_GUEST_MUTATION_PACKAGE_SHA256 = $pin.package_sha256
    foreach ($point in $points) {
      Invoke-ContentCase $pin.language 'native' $point $true
    }
    Invoke-ContentCase $pin.language 'native' 'file-content-after-commit' $false
    if ($pin.language -eq 'rust') {
      foreach ($point in $points) {
        Invoke-ContentCase 'rust' 'widget' $point $true
      }
      Invoke-ContentCase 'rust' 'widget' 'file-content-before-commit' $false
    }
    if ((Get-FileHash -LiteralPath $pin.module -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pin.module_sha256 -or
        (Get-FileHash -LiteralPath $pin.package -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pin.package_sha256) {
      throw "$($pin.language) SDK module or package changed during matrix."
    }
  }
  if ($cases.Count -ne 20 -or
      @($cases | Where-Object suite -eq 'native').Count -ne 15 -or
      @($cases | Where-Object suite -eq 'widget').Count -ne 5) {
    throw "Incomplete Core content transaction matrix: $($cases.Count) cases."
  }
  foreach ($source in $sources) {
    if ((Get-FileHash -LiteralPath $source.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $source.sha256) {
      throw "Source changed during Core content transaction matrix: $($source.path)."
    }
  }
  if ((Get-FileHash -LiteralPath $priorPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $priorHash -or
      (Get-FileHash -LiteralPath $normalHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $normalHash -or
      (Get-FileHash -LiteralPath $faultHost -Algorithm SHA256).Hash.ToLowerInvariant() -ne $faultHash -or
      (Get-FileHash -LiteralPath $builtin -Algorithm SHA256).Hash.ToLowerInvariant() -ne $builtinHash -or
      (Get-FileHash -LiteralPath $storeVerifier -Algorithm SHA256).Hash.ToLowerInvariant() -ne $storeVerifierHash) {
    throw 'Pinned prior qualification, host, builtin, or Store verifier changed during matrix.'
  }
  $success = [ordered]@{
    status = 'passed'; started_utc = $started
    scope = 'Windows real SDK 16 MiB guest Prepare-only Store content transaction process exits at four Core points, normal-host controls, and Rust recovery Widget; no Execute, file-effect fault, or power-loss claim'
    profile = 'content-transaction-maximum-v1'
    content_bytes = 16777216; chunk_bytes = 61440; chunks = 274
    prior_qualification = $priorPath; prior_qualification_sha256 = $priorHash
    normal_host_sha256 = $normalHash; fault_host_sha256 = $faultHash
    builtin_package_sha256 = $builtinHash
    store_verifier_sha256 = $storeVerifierHash
    run_directory = $run; source_sha256 = $sources
    guests = $artifactPins; cases = $cases
  }
  $success | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $summaryPath -Encoding utf8
  $success | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $run 'summary-passed.json') -Encoding utf8
  Write-Output 'PASS: 12 Core content transaction SDK crash cases, 3 normal-host SDK controls, 4 Rust Widget crash cases, and 1 Rust Widget control.'
} catch {
  if ($evidenceRoot) {
    $failed = [ordered]@{
      status = 'failed'; started_utc = $started
      run_directory = $run; cases_passed = $cases.Count; error = $_.Exception.Message
    }
    $failed | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidenceRoot 'summary.json') -Encoding utf8
    if ($run -and (Test-Path -LiteralPath $run)) {
      $failed | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $run 'summary-failed.json') -Encoding utf8
    }
  }
  throw
} finally {
  foreach ($name in $variables) {
    [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process')
  }
  Pop-Location
}
