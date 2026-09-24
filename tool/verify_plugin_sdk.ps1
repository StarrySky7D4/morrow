param([string]$Clang='clang',[string]$ClangXX='clang++',[string]$Ar='llvm-ar',[string]$Python='python')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){ & $Program @Arguments; if($LASTEXITCODE -ne 0){throw "SDK verification failed: $Program"} }
Push-Location $repo
try {
 & ./tool/verify_plugin_sdk_compat.ps1 -Python $Python
 New-Item -ItemType Directory -Force build/plugin-sdk | Out-Null
 Checked $Python @('tool/sync_plugin_sdk_contracts.py','--check')
 Checked cargo @('fmt','--manifest-path','sdk/rust/Cargo.toml','--check')
 Checked cargo @('clippy','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust','--all-targets','--','-D','warnings')
 Checked cargo @('test','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust')
 Checked cargo @('check','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust','--target','wasm32-unknown-unknown')
 Checked cargo @('build','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust')
 Copy-Item -LiteralPath 'build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll' -Destination 'build/plugin-sdk/morrow_plugin_sdk.dll' -Force
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','-c','sdk/c/src/morrow_plugin_sdk.c','-o','build/plugin-sdk/morrow_plugin_sdk.obj')
 Checked $Ar @('rcs','build/plugin-sdk/morrow_plugin_sdk.lib','build/plugin-sdk/morrow_plugin_sdk.obj')
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','sdk/tests/c_transport.c','build/plugin-sdk/morrow_plugin_sdk.lib','-o','build/plugin-sdk/c_transport.exe')
 Checked './build/plugin-sdk/c_transport.exe' @()
 Checked $ClangXX @('-std=c++17','-Wall','-Wextra','-Werror','-Isdk/c/include','-Isdk/cpp/include','sdk/tests/cpp_transport.cpp','build/plugin-sdk/morrow_plugin_sdk.lib','-o','build/plugin-sdk/cpp_transport.exe')
 Checked './build/plugin-sdk/cpp_transport.exe' @()
 Checked $ClangXX @('-std=c++17','-Wall','-Wextra','-Werror','-Isdk/c/include','-Isdk/cpp/include','sdk/tests/cpp_codec.cpp','build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll.lib','-o','build/plugin-sdk/cpp_codec.exe')
 Checked './build/plugin-sdk/cpp_codec.exe' @('sdk/tests/fixtures')
 # IO is experimental and independently qualified; keep old SDK originals unchanged.
 $ioFixtures=Join-Path $repo 'build/plugin-sdk/io-smoke'
 $priorIoFixtures=$env:MORROW_IO_SMOKE_DIR
 try {
  $env:MORROW_IO_SMOKE_DIR=$ioFixtures
  Checked cargo @('test','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust','--test','io_ffi','c_digest_and_optional_native_fixture')
 } finally {$env:MORROW_IO_SMOKE_DIR=$priorIoFixtures}
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','sdk/tests/c_io.c','build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll.lib','-o','build/plugin-sdk/c_io.exe')
 Checked './build/plugin-sdk/c_io.exe' @((Join-Path $ioFixtures 'request.capnp'),(Join-Path $ioFixtures 'response.capnp'))
 Checked $ClangXX @('-std=c++17','-Wall','-Wextra','-Werror','-Isdk/c/include','-Isdk/cpp/include','sdk/tests/cpp_io.cpp','build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll.lib','-o','build/plugin-sdk/cpp_io.exe')
 Checked './build/plugin-sdk/cpp_io.exe' @((Join-Path $ioFixtures 'request.capnp'),(Join-Path $ioFixtures 'response.capnp'))
 if(-not $IsWindows){throw 'Native adapter qualification currently requires Windows'}
 Checked cargo @('build','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/plugin-sdk/core','--lib','--bin','morrow-core-store')
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/plugin-sdk/core','--example','sdk_codec_vectors','--','build/plugin-sdk/codec-vectors')
 foreach($fixture in Get-ChildItem -LiteralPath 'sdk/tests/fixtures' -Filter '*.capnp') {
  $actual=Join-Path 'build/plugin-sdk/codec-vectors' $fixture.Name
  if((Get-FileHash -LiteralPath $fixture.FullName).Hash -ne (Get-FileHash -LiteralPath $actual).Hash){throw "Stale SDK fixture: $($fixture.Name)"}
 }

 $caseDir=Join-Path $repo ('build/plugin-sdk/native-'+[guid]::NewGuid().ToString('N'))
 New-Item -ItemType Directory -Path $caseDir | Out-Null
 $db=Join-Path $caseDir 'sdk.db'
 Checked './build/plugin-sdk/core/debug/morrow-core-store.exe' @('init',$db)
 Checked './build/plugin-sdk/core/debug/morrow-core-store.exe' @('create-local',$db,'seed','legacy-123','old')
 Checked $Clang @('-std=c11','-Wall','-Wextra','-Werror','-Isdk/c/include','sdk/tests/native_adapter.c','build/plugin-sdk/morrow_plugin_sdk.lib','build/plugin-sdk/rust/debug/morrow_plugin_sdk.dll.lib','-o','build/plugin-sdk/native_adapter.exe')
 Checked './build/plugin-sdk/native_adapter.exe' @((Join-Path $repo 'build/plugin-sdk/core/debug/morrow_core.dll'),$db,$caseDir)
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/plugin-sdk/core','--example','sdk_transport_check','--',$caseDir)
 Checked './build/plugin-sdk/core/debug/morrow-core-store.exe' @('check',$db)
} finally {Pop-Location}
