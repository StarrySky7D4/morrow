param([string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0',[string]$Output='build/dynamic-sdk/wasm')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "Dependency SDK build failed: $Program"}}
Push-Location $repo
try {
 $dependencySysroot=(Resolve-Path -LiteralPath $Sysroot).Path
 New-Item -ItemType Directory -Force $Output | Out-Null
 $rustTarget=Join-Path $Output 'rust'
 Checked cargo @('rustc','--offline','--locked','--manifest-path','sdk/rust/Cargo.toml','--target','wasm32-unknown-unknown','--features','wasm-c','--release','--target-dir',$rustTarget,'--crate-type','staticlib')
 $common=@('--target=wasm32-wasip1',"--sysroot=$dependencySysroot",'-O2','-Wall','-Wextra','-Werror','-Isdk/c/include')
 $objects=@()
 foreach($name in @('morrow_plugin_wasm_libc','morrow_plugin_task')){
  $obj=Join-Path $Output "$name.o"
  Checked clang ($common+@('-std=c11','-c',"sdk/c/src/$name.c",'-o',$obj))
  $objects+=$obj
 }
 $codec=Join-Path $rustTarget 'wasm32-unknown-unknown/release/libmorrow_plugin_sdk.a'
 $link=@('-nostdlib','-Wl,--no-entry','-Wl,--export=morrow_run','-Wl,-z,stack-size=1048576','-Wl,--max-memory=16777216','-Wl,--strip-all')
 $stdlib=Join-Path $dependencySysroot 'lib/wasm32-wasip1'
 Checked clang ($common+@('-std=c11','sdk/examples/c-dependency-caller/plugin.c')+$objects+@($codec)+$link+@("-L$stdlib",'-lc','-o',(Join-Path $Output 'c_dependency_caller.wasm')))
 $cppLib=Join-Path $stdlib 'noeh'
 $cppIncludes=Join-Path $dependencySysroot 'include/wasm32-wasip1/noeh/c++/v1'
 $cppCommon=$common+@('-std=c++17','-nostdinc++','-isystem',$cppIncludes,'-fno-exceptions','-fno-rtti','-Isdk/cpp/include')
 $cppRuntime=Join-Path $Output 'morrow_plugin_wasm_runtime.o'
 Checked clang++ ($cppCommon+@('-c','sdk/cpp/src/morrow_plugin_wasm_runtime.cpp','-o',$cppRuntime))
 Checked clang++ ($cppCommon+@('-Dmorrow_run=mp_guest_run','sdk/examples/cpp-dependency-caller/plugin.cpp',$cppRuntime)+$objects+@($codec)+$link+@('-Wl,--export=__wasm_call_ctors',"-L$cppLib","-L$stdlib",'-lc++','-lc++abi','-lc','-o',(Join-Path $Output 'cpp_dependency_caller.wasm')))
}finally{Pop-Location}
