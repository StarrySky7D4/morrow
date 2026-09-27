param(
  [string]$Python = 'python',
  [string]$Clang = 'clang',
  [string]$ClangXX = 'clang++',
  [string]$RustTarget = 'sdk/rust/target',
  [string]$OutputDirectory = 'build/mutation-sdk-native'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'This native DLL qualification currently supports Windows only.'
}
$repo = Split-Path $PSScriptRoot -Parent
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Mutation SDK verification failed: $Program (exit $LASTEXITCODE)" }
}
Push-Location $repo
try {
  Checked $Python @('tool/sync_plugin_sdk_contracts.py', '--check')
  Checked $Python @('-m', 'unittest', 'tool.tests.test_mutation_vectors')
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Checked cargo @('build', '--locked', '--offline', '--manifest-path', 'sdk/rust/Cargo.toml', '--target-dir', $RustTarget, '--lib')
  $nativeOutput = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
  New-Item -ItemType Directory -Force -Path $nativeOutput | Out-Null
  $dll = Join-Path $RustTarget 'debug/morrow_plugin_sdk.dll'
  $importLibrary = Join-Path $RustTarget 'debug/morrow_plugin_sdk.dll.lib'
  Copy-Item -LiteralPath $dll -Destination (Join-Path $nativeOutput 'morrow_plugin_sdk.dll') -Force
  $cExe = Join-Path $nativeOutput 'c_mutation.exe'
  $cppExe = Join-Path $nativeOutput 'cpp_mutation.exe'
  Checked $Clang @('-std=c11', '-Wall', '-Wextra', '-Werror', '-Isdk/c/include', 'sdk/tests/c_mutation.c', $importLibrary, '-o', $cExe)
  Checked $ClangXX @('-std=c++17', '-Wall', '-Wextra', '-Werror', '-Isdk/c/include', '-Isdk/cpp/include', 'sdk/tests/cpp_mutation.cpp', $importLibrary, '-o', $cppExe)
  $vectors = (Resolve-Path 'sdk/vectors/mutation-v1').Path
  Checked $cExe @($vectors)
  Checked $cppExe @($vectors)
  Checked $Python @('tool/plugin_transport_baseline.py', 'verify')
  Write-Output 'PASS: native C/C++ mutation codecs and shared vectors. No guest import or OS effect executed.'
} finally {
  Pop-Location
}
