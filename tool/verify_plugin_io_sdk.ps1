param([string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0', [string]$Python='python')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "IO SDK verification failed: $Program"}}
Push-Location $repo
$oldRust=$env:MORROW_RUST_IO_WASM
$oldC=$env:MORROW_SDK_IO_GUEST_C
$oldCpp=$env:MORROW_SDK_IO_GUEST_CPP
try {
 & ./tool/verify_plugin_sdk_compat.ps1 -Python $Python
 & ./tool/build_plugin_io_wasm.ps1 -Sysroot $Sysroot
 Checked $Python @('tool/sync_plugin_sdk_contracts.py','--check')
 $env:MORROW_RUST_IO_WASM=(Resolve-Path 'build/sdk-io-guest/wasm32-unknown-unknown/release/morrow_example_io.wasm').Path
 $env:MORROW_SDK_IO_GUEST_C=(Resolve-Path 'build/io-sdk/wasm/c_io.wasm').Path
 $env:MORROW_SDK_IO_GUEST_CPP=(Resolve-Path 'build/io-sdk/wasm/cpp_io.wasm').Path
 Checked cargo @('test','--offline','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/sdk-io-host','--features','packages','--test','sdk_io_guest','--','--ignored','--nocapture')
} finally {
 $env:MORROW_RUST_IO_WASM=$oldRust
 $env:MORROW_SDK_IO_GUEST_C=$oldC
 $env:MORROW_SDK_IO_GUEST_CPP=$oldCpp
 Pop-Location
}
