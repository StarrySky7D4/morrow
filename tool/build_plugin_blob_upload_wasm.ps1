param(
  [Parameter(Mandatory=$true)][string]$Sysroot,
  [Parameter(Mandatory=$true)][string]$Output,
  [string]$Clang = 'clang',
  [string]$ClangXX = 'clang++'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Upload guest build failed: $Program (exit $LASTEXITCODE)" }
}
Push-Location $repo
try {
  $uploadSysroot = (Resolve-Path -LiteralPath $Sysroot).Path
  $outputPath = [IO.Path]::GetFullPath($Output)
  if (-not $outputPath.StartsWith('D:\build_cache\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'This Windows build requires an output under D:\build_cache'
  }
  if (Test-Path -LiteralPath $outputPath) { throw 'Use a fresh output directory' }
  New-Item -ItemType Directory -Path $outputPath | Out-Null
  $rustTarget = Join-Path $outputPath 'rust'
  Checked cargo @('build','--offline','--locked','--manifest-path','sdk/examples/rust-blob-upload/Cargo.toml','--target','wasm32-unknown-unknown','--release','--jobs','1','--target-dir',$rustTarget)
  Copy-Item -LiteralPath (Join-Path $rustTarget 'wasm32-unknown-unknown/release/morrow_example_blob_upload.wasm') -Destination (Join-Path $outputPath 'rust_upload.wasm')
  Checked cargo @('build','--offline','--locked','--manifest-path','sdk/wasm/blob-upload/Cargo.toml','--target','wasm32-unknown-unknown','--release','--jobs','1','--target-dir',$rustTarget)
  $common = @('--target=wasm32-wasip1', "--sysroot=$uploadSysroot", '-O2', '-Wall', '-Wextra', '-Werror', '-Isdk/c/include', '-Iextensions/blob-transfer-v1/include', '-Iextensions/blob-upload-request-v1/include', '-Iextensions/blob-upload-proposal-v1/include')
  $objects = @()
  foreach ($source in @('sdk/c/src/morrow_plugin_wasm_libc.c','sdk/c/src/morrow_plugin_task.c','extensions/blob-upload-request-v1/wasm/morrow_blob_upload_wasm.c')) {
    $objectPath = Join-Path $outputPath (([IO.Path]::GetFileNameWithoutExtension($source)) + '.o')
    Checked $Clang ($common + @('-std=c11','-c',$source,'-o',$objectPath))
    $objects += $objectPath
  }
  $codec = Join-Path $rustTarget 'wasm32-unknown-unknown/release/libmorrow_upload_wasm_codecs.a'
  $link = @('-nostdlib','-Wl,--no-entry','-Wl,--export=morrow_run','-Wl,-z,stack-size=1048576','-Wl,--max-memory=16777216','-Wl,--strip-all')
  $stdlib = Join-Path $uploadSysroot 'lib/wasm32-wasip1'
  Checked $Clang ($common + @('-std=c11','sdk/examples/c-blob-upload/plugin.c') + $objects + @($codec) + $link + @("-L$stdlib",'-lc','-o',(Join-Path $outputPath 'c_upload.wasm')))
  $cppLib = Join-Path $stdlib 'noeh'
  $cppIncludes = Join-Path $uploadSysroot 'include/wasm32-wasip1/noeh/c++/v1'
  $cppCommon = $common + @('-std=c++17','-nostdinc++','-isystem',$cppIncludes,'-fno-exceptions','-fno-rtti')
  $cppRuntime = Join-Path $outputPath 'morrow_plugin_wasm_runtime.o'
  Checked $ClangXX ($cppCommon + @('-c','sdk/cpp/src/morrow_plugin_wasm_runtime.cpp','-o',$cppRuntime))
  Checked $ClangXX ($cppCommon + @('-Dmorrow_run=mp_guest_run','sdk/examples/cpp-blob-upload/plugin.cpp',$cppRuntime) + $objects + @($codec) + $link + @('-Wl,--export=__wasm_call_ctors',"-L$cppLib","-L$stdlib",'-lc++','-lc++abi','-lc','-o',(Join-Path $outputPath 'cpp_upload.wasm')))
  foreach ($name in @('rust_upload.wasm','c_upload.wasm','cpp_upload.wasm')) {
    Get-FileHash -LiteralPath (Join-Path $outputPath $name) -Algorithm SHA256
  }
} finally { Pop-Location }
