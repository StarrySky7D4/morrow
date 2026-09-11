param(
 [string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0',
 [string]$Clang='clang', [string]$ClangXX='clang++', [switch]$DebugSymbols
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){ & $Program @Arguments; if($LASTEXITCODE -ne 0){throw "C/C++ guest build failed: $Program"} }
Push-Location $repo
try {
 $resolvedSysroot=(Resolve-Path -LiteralPath $Sysroot).Path
 $output='build/plugin-c-guest'
 New-Item -ItemType Directory -Force $output | Out-Null
 Checked cargo @('rustc','--locked','--manifest-path','sdk/rust/Cargo.toml','--target','wasm32-unknown-unknown','--features','wasm-c','--release','--target-dir','build/plugin-guest','--crate-type','staticlib')
 $common=@('--target=wasm32-wasip1',"--sysroot=$resolvedSysroot",'-O2','-Wall','-Wextra','-Werror','-Isdk/c/include')
 $objects=@()
 foreach($name in @('morrow_plugin_sdk','morrow_plugin_wasm','morrow_plugin_wasm_libc','morrow_plugin_task')) {
  $obj="$output/$name.o"
  Checked $Clang ($common+@('-std=c11','-c',"sdk/c/src/$name.c",'-o',$obj))
  $objects+=$obj
 }
 $link=@('-nostdlib','-Wl,--no-entry','-Wl,--export=morrow_run','-Wl,-z,stack-size=1048576','-Wl,--max-memory=16777216')
 if(-not $DebugSymbols){$link+='-Wl,--strip-all'}
 $stdlib=Join-Path $resolvedSysroot 'lib/wasm32-wasip1'
 $cppLib=Join-Path $stdlib 'noeh'
 $cppIncludes=Join-Path $resolvedSysroot 'include/wasm32-wasip1/noeh/c++/v1'
 $codec='build/plugin-guest/wasm32-unknown-unknown/release/libmorrow_plugin_sdk.a'
 Checked $Clang ($common+@('-std=c11','sdk/examples/c-rename/plugin.c')+$objects+@($codec)+$link+@("-L$stdlib",'-lc','-o',"$output/c_rename.wasm"))
 Checked $Clang ($common+@('-std=c11','sdk/examples/c-task/plugin.c')+$objects+@($codec)+$link+@("-L$stdlib",'-lc','-o',"$output/c_task.wasm"))
 Checked $Clang ($common+@('-std=c11','sdk/examples/c-transform/plugin.c')+$objects+@($codec)+$link+@("-L$stdlib",'-lc','-o',"$output/c_transform.wasm"))
 Checked $Clang ($common+@('-std=c11','sdk/examples/c-ui/plugin.c')+$objects+@($codec)+$link+@("-L$stdlib",'-lc','-o',"$output/c_ui.wasm"))
 $cppCommon=$common+@('-std=c++17','-nostdinc++','-isystem',$cppIncludes,'-fno-exceptions','-fno-rtti','-Isdk/cpp/include')
 $cppRuntime="$output/morrow_plugin_wasm_runtime.o"
 Checked $ClangXX ($cppCommon+@('-c','sdk/cpp/src/morrow_plugin_wasm_runtime.cpp','-o',$cppRuntime))
 foreach($entry in @(@('sdk/examples/cpp-rename/plugin.cpp','cpp_rename'),@('sdk/examples/cpp-task/plugin.cpp','cpp_task'),@('sdk/examples/cpp-transform/plugin.cpp','cpp_transform'),@('sdk/examples/cpp-ui/plugin.cpp','cpp_ui'),@('sdk/tests/wasm_allocator.cpp','cpp_allocator'),@('sdk/tests/wasm_cpp_abort.cpp','cpp_abort'),@('sdk/tests/wasm_cpp_oom.cpp','cpp_oom'))) {
  Checked $ClangXX ($cppCommon+@('-Dmorrow_run=mp_guest_run',$entry[0],$cppRuntime)+$objects+@($codec)+$link+@('-Wl,--export=__wasm_call_ctors',"-L$cppLib","-L$stdlib",'-lc++','-lc++abi','-lc','-o',"$output/$($entry[1]).wasm"))
 }
} finally {Pop-Location}
