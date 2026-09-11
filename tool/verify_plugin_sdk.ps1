param([string]$Clang='clang',[string]$ClangXX='clang++',[string]$Ar='llvm-ar')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){ & $Program @Arguments; if($LASTEXITCODE -ne 0){throw "SDK verification failed: $Program"} }
Push-Location $repo
try {
 New-Item -ItemType Directory -Force build/plugin-sdk | Out-Null
 Checked cargo @('fmt','--manifest-path','sdk/rust/Cargo.toml','--check')
 Checked cargo @('clippy','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust','--all-targets','--','-D','warnings')
 Checked cargo @('test','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust')
 Checked cargo @('check','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust','--target','wasm32-unknown-unknown')
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','-c','sdk/c/src/morrow_plugin_sdk.c','-o','build/plugin-sdk/morrow_plugin_sdk.obj')
 Checked $Ar @('rcs','build/plugin-sdk/morrow_plugin_sdk.lib','build/plugin-sdk/morrow_plugin_sdk.obj')
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','sdk/tests/c_transport.c','build/plugin-sdk/morrow_plugin_sdk.lib','-o','build/plugin-sdk/c_transport.exe')
 Checked './build/plugin-sdk/c_transport.exe' @()
 Checked $ClangXX @('-std=c++17','-Wall','-Wextra','-Werror','-Isdk/c/include','-Isdk/cpp/include','sdk/tests/cpp_transport.cpp','build/plugin-sdk/morrow_plugin_sdk.lib','-o','build/plugin-sdk/cpp_transport.exe')
 Checked './build/plugin-sdk/cpp_transport.exe' @()
 if(-not $IsWindows){throw 'Native adapter qualification currently requires Windows'}
 Checked cargo @('build','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/plugin-sdk/core','--lib','--bin','morrow-core-store')
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/plugin-sdk/core','--example','protocol_vectors','--','build/plugin-sdk/vectors')
 $caseDir=Join-Path $repo ('build/plugin-sdk/native-'+[guid]::NewGuid().ToString('N'))
 New-Item -ItemType Directory -Path $caseDir | Out-Null
 $db=Join-Path $caseDir 'sdk.db'
 Checked './build/plugin-sdk/core/debug/morrow-core-store.exe' @('init',$db)
 Checked './build/plugin-sdk/core/debug/morrow-core-store.exe' @('create-local',$db,'seed','legacy-123','old')
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','sdk/tests/native_adapter.c','build/plugin-sdk/morrow_plugin_sdk.lib','-o','build/plugin-sdk/native_adapter.exe')
 Checked './build/plugin-sdk/native_adapter.exe' @((Join-Path $repo 'build/plugin-sdk/core/debug/morrow_core.dll'),$db,(Join-Path $repo 'build/plugin-sdk/vectors/one.capnp'),$caseDir)
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/plugin-sdk/core','--example','sdk_transport_check','--',$caseDir)
 Checked './build/plugin-sdk/core/debug/morrow-core-store.exe' @('check',$db)
} finally {Pop-Location}
