param(
  [string]$Sysroot = 'build/tools/wasi-34/wasi-sysroot-34.0',
  [string]$Output = 'build/mutation-sdk-wasm',
  [string]$Python = 'python'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Mutation Wasm qualification failed: $Program (exit $LASTEXITCODE)" }
}
Push-Location $repo
$priorDirectory = $env:MORROW_MUTATION_SDK_WASM_DIR
try {
  Checked $Python @('tool/sync_plugin_sdk_contracts.py', '--check')
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked $Python @('tool/verify_plugin_sdk_baseline.py')
  & ./tool/build_plugin_mutation_wasm.ps1 -Sysroot $Sysroot -Output $Output
  $env:MORROW_MUTATION_SDK_WASM_DIR = (Resolve-Path -LiteralPath $Output).Path
  # Qualify the real 30-second lease against an optimized native host as shipped.
  # Debug-host timing failures must be retained separately, never hidden by a
  # synthetic clock or a longer binding lifetime.
  $testArguments = @('test', '--release', '--locked', '--offline', '--manifest-path', 'plugin_runtime/Cargo.toml', '--features', 'packages', '--test', 'mutation_owner', 'mutation_sdk_wasm')
  # Cargo returns success when a filter matches zero tests. Require all six
  # registered qualification tests before allowing the execution to report PASS.
  $listedTests = & cargo @testArguments -- --list --ignored
  if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate mutation SDK qualification tests.' }
  $actualTests = @($listedTests | Where-Object { $_ -match ': test$' } | Sort-Object)
  $expectedTests = @('c', 'cpp', 'rust') | ForEach-Object {
    "mutation_sdk_wasm::$($_)_mutation_sdk_uses_original_approved_owner: test"
    "mutation_sdk_wasm::$($_)_mutation_sdk_creates_maximum_content: test"
  } | Sort-Object
  if ($actualTests.Count -ne 6 -or (Compare-Object $expectedTests $actualTests)) {
    throw "Expected exactly six compiled SDK qualification tests, found: $($actualTests -join ', ')"
  }
  Checked cargo ($testArguments + @('--', '--ignored', '--test-threads=1', '--nocapture'))
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked $Python @('tool/verify_plugin_sdk_baseline.py')
  Write-Output 'PASS: Release native host and three compiled SDK guests, ordinary and maximum-content flows, through the approved original Windows owner; this is not full SDK freeze or cross-platform qualification.'
} finally {
  $env:MORROW_MUTATION_SDK_WASM_DIR = $priorDirectory
  Pop-Location
}
