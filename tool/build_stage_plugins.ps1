param([string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0')
$ErrorActionPreference='Stop'
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "Stage plugin build failed: $Program"}}
$repo=Split-Path $PSScriptRoot -Parent
Push-Location $repo
try{
 & ./tool/build_plugin_c_wasm.ps1 -Sysroot $Sysroot
 $resolved=(Resolve-Path -LiteralPath $Sysroot).Path
 $out='build/plugin-stage-guest'
 New-Item -ItemType Directory -Force $out | Out-Null
 $common=@('--target=wasm32-wasip1',"--sysroot=$resolved",'-O2','-Wall','-Wextra','-Werror','-Isdk/c/include')
 $objects=@('morrow_plugin_sdk','morrow_plugin_wasm','morrow_plugin_wasm_libc','morrow_plugin_task') | ForEach-Object {"build/plugin-c-guest/$_.o"}
 $codec='build/plugin-guest/wasm32-unknown-unknown/release/libmorrow_plugin_sdk.a'
 $link=@('-nostdlib','-Wl,--no-entry','-Wl,--export=morrow_run','-Wl,-z,stack-size=1048576','-Wl,--max-memory=16777216','-Wl,--strip-all')
 $stdlib=Join-Path $resolved 'lib/wasm32-wasip1'
 Checked clang ($common+@('-std=c11','demos/plugin_stage_windows/plugins/c/plugin.c')+$objects+@($codec)+$link+@("-L$stdlib",'-lc','-o',"$out/c_units.wasm"))
 $cpp=$common+@('-std=c++17','-nostdinc++','-isystem',(Join-Path $resolved 'include/wasm32-wasip1/noeh/c++/v1'),'-fno-exceptions','-fno-rtti','-Isdk/cpp/include')
 Checked clang++ ($cpp+@('-Dmorrow_run=mp_guest_run','demos/plugin_stage_windows/plugins/cpp/plugin.cpp','build/plugin-c-guest/morrow_plugin_wasm_runtime.o')+$objects+@($codec)+$link+@('-Wl,--export=__wasm_call_ctors',"-L$stdlib/noeh","-L$stdlib",'-lc++','-lc++abi','-lc','-o',"$out/cpp_tasks.wasm"))
}finally{Pop-Location}
