param(
  [string]$Sysroot = 'build/tools/wasi-34/wasi-sysroot-34.0',
  [string]$Output = 'build/mutation-sdk-wasm',
  [string]$Clang = 'clang',
  [string]$ClangXX = 'clang++'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Mutation guest build failed: $Program (exit $LASTEXITCODE)" }
}
Push-Location $repo
try {
  $mutationSysroot = (Resolve-Path -LiteralPath $Sysroot).Path
  $outputPath = if ([IO.Path]::IsPathRooted($Output)) {
    [IO.Path]::GetFullPath($Output)
  } else {
    [IO.Path]::GetFullPath((Join-Path $repo $Output))
  }
  New-Item -ItemType Directory -Force -Path $outputPath | Out-Null
  $rustTarget = Join-Path $outputPath 'rust'
  Checked cargo @('build', '--offline', '--locked', '--manifest-path', 'sdk/examples/rust-mutation/Cargo.toml', '--target', 'wasm32-unknown-unknown', '--release', '--target-dir', $rustTarget)
  Copy-Item -LiteralPath (Join-Path $rustTarget 'wasm32-unknown-unknown/release/morrow_example_mutation.wasm') -Destination (Join-Path $outputPath 'rust_mutation.wasm') -Force
  Checked cargo @('rustc', '--offline', '--locked', '--manifest-path', 'sdk/rust/Cargo.toml', '--target', 'wasm32-unknown-unknown', '--features', 'wasm-c', '--release', '--target-dir', $rustTarget, '--crate-type', 'staticlib')
  $common = @('--target=wasm32-wasip1', "--sysroot=$mutationSysroot", '-O2', '-Wall', '-Wextra', '-Werror', '-Isdk/c/include')
  $objects = @()
  foreach ($name in @('morrow_plugin_wasm_libc', 'morrow_plugin_task')) {
    $objectPath = Join-Path $outputPath "$name.o"
    Checked $Clang ($common + @('-std=c11', '-c', "sdk/c/src/$name.c", '-o', $objectPath))
    $objects += $objectPath
  }
  $codec = Join-Path $rustTarget 'wasm32-unknown-unknown/release/libmorrow_plugin_sdk.a'
  $link = @('-nostdlib', '-Wl,--no-entry', '-Wl,--export=morrow_run', '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=16777216', '-Wl,--strip-all')
  $stdlib = Join-Path $mutationSysroot 'lib/wasm32-wasip1'
  Checked $Clang ($common + @('-std=c11', 'sdk/examples/c-mutation/plugin.c') + $objects + @($codec) + $link + @("-L$stdlib", '-lc', '-o', (Join-Path $outputPath 'c_mutation.wasm')))
  $cppLib = Join-Path $stdlib 'noeh'
  $cppIncludes = Join-Path $mutationSysroot 'include/wasm32-wasip1/noeh/c++/v1'
  $cppCommon = $common + @('-std=c++17', '-nostdinc++', '-isystem', $cppIncludes, '-fno-exceptions', '-fno-rtti', '-Isdk/cpp/include')
  $cppRuntime = Join-Path $outputPath 'morrow_plugin_wasm_runtime.o'
  Checked $ClangXX ($cppCommon + @('-c', 'sdk/cpp/src/morrow_plugin_wasm_runtime.cpp', '-o', $cppRuntime))
  Checked $ClangXX ($cppCommon + @('-Dmorrow_run=mp_guest_run', 'sdk/examples/cpp-mutation/plugin.cpp', $cppRuntime) + $objects + @($codec) + $link + @('-Wl,--export=__wasm_call_ctors', "-L$cppLib", "-L$stdlib", '-lc++', '-lc++abi', '-lc', '-o', (Join-Path $outputPath 'cpp_mutation.wasm')))
  $hashes = foreach ($name in @('rust_mutation.wasm', 'c_mutation.wasm', 'cpp_mutation.wasm')) {
    $digest = (Get-FileHash -LiteralPath (Join-Path $outputPath $name) -Algorithm SHA256).Hash.ToLowerInvariant()
    "$digest  $name"
  }
  $hashes | Set-Content -LiteralPath (Join-Path $outputPath 'SHA256SUMS') -Encoding ascii
  $hashes | Write-Output
  Write-Output 'Built three mutation guests. Compilation alone does not qualify host authorization or file effects.'
} finally {
  Pop-Location
}
