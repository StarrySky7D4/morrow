param([string]$Python='python',[string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "UI SDK verification failed: $Program"}}
Push-Location $repo
try {
 Checked $Python @('tool/sync_plugin_sdk_contracts.py','--check')
 foreach($manifest in @('sdk/rust/Cargo.toml','sdk/examples/rust-ui/Cargo.toml','plugin_runtime/Cargo.toml')){Checked cargo @('fmt','--manifest-path',$manifest,'--check')}
 Checked cargo @('clippy','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust','--all-targets','--','-D','warnings')
 Checked cargo @('test','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust')
 Checked cargo @('build','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust')
 New-Item -ItemType Directory -Force build/plugin-ui-sdk | Out-Null
 Copy-Item -LiteralPath 'build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll' -Destination 'build/plugin-ui-sdk/morrow_plugin_sdk.dll' -Force
 Checked clang++ @('-std=c++17','-Wall','-Wextra','-Werror','-Isdk/c/include','-Isdk/cpp/include','sdk/tests/cpp_ui.cpp','build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll.lib','-o','build/plugin-ui-sdk/cpp_ui.exe')
 Checked './build/plugin-ui-sdk/cpp_ui.exe' @('sdk/tests/ui_fixtures/event.capnp')
 Checked cargo @('clippy','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/plugin-runtime','--all-targets','--all-features','--','-D','warnings')
 Checked cargo @('test','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/plugin-runtime','--all-features')
 Checked cargo @('clippy','--locked','--manifest-path','sdk/rust/Cargo.toml','--features','wasm-c','--target','wasm32-unknown-unknown','--target-dir','build/plugin-sdk/rust','--','-D','warnings')
 Checked cargo @('clippy','--locked','--manifest-path','sdk/examples/rust-ui/Cargo.toml','--target','wasm32-unknown-unknown','--target-dir','build/plugin-guest','--','-D','warnings')
 Checked cargo @('build','--locked','--manifest-path','sdk/examples/rust-ui/Cargo.toml','--target','wasm32-unknown-unknown','--release','--target-dir','build/plugin-guest')
 & ./tool/build_plugin_c_wasm.ps1 -Sysroot $Sysroot
 # Produces a fresh event from an actual Flutter input, then checks it in Rust.
 & ./tool/verify_plugin_renderer.ps1
 foreach($pair in @(@('document.capnp','document.capnp'),@('expected-event.capnp','event.capnp'))){
  if((Get-FileHash (Join-Path 'build/ui-protocol' $pair[0])).Hash -ne (Get-FileHash (Join-Path 'sdk/tests/ui_fixtures' $pair[1])).Hash){throw 'Stale guest UI fixture'}
 }
 $packageDir=Join-Path 'build/plugin-ui-sdk/packages' ([guid]::NewGuid().ToString('N'))
 New-Item -ItemType Directory -Force $packageDir | Out-Null
 $packages=@()
 foreach($guest in @(@('rust-ui','build/plugin-guest/wasm32-unknown-unknown/release/morrow_example_ui.wasm'),@('c-ui','build/plugin-c-guest/c_ui.wasm'),@('cpp-ui','build/plugin-c-guest/cpp_ui.wasm'))){
  $package=Join-Path $packageDir ($guest[0]+'.mplugin')
  Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','plugin_package','--','pack-transform',$guest[1],$package,('org.morrow.example.'+$guest[0]),'0.1.9-test.10','ui.form,text.utf8,morrow.ui.document.v1,32,65536;ui.edit,morrow.ui.event.v1,morrow.ui.document.v1,65536,65536')
  $packages+=$package
 }
 Checked cargo (@('run','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/plugin-runtime','--features','packages','--example','qualify_ui','--','build/plugin-ui-sdk/artifacts','build/ui-protocol/widget-event.capnp')+$packages)
 Push-Location packages/morrow_plugin_ui
 try{Checked flutter @('test','--no-pub','test/guest_form_test.dart')}finally{Pop-Location}
 Get-FileHash -Algorithm SHA256 -LiteralPath $packages
 Get-ChildItem build/plugin-ui-sdk/artifacts -Filter '*.capnp' | Get-FileHash -Algorithm SHA256
}finally{Pop-Location}
