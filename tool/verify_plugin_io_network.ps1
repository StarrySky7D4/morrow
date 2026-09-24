param([string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0', [string]$Python='python', [string]$RustPackage='', [string]$CPackage='', [string]$CppPackage='')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "IO network verification failed: $Program"}}
function Set-ProcessEnv([string]$Key,$Value){
 if($null -eq $Value){Remove-Item -LiteralPath "Env:$Key" -ErrorAction SilentlyContinue}
 else {Set-Item -LiteralPath "Env:$Key" -Value $Value}
}
Push-Location $repo
$oldRust=$env:MORROW_RUST_IO_WASM
$oldC=$env:MORROW_SDK_IO_GUEST_C
$oldCpp=$env:MORROW_SDK_IO_GUEST_CPP
$oldPackageEnv=@{}
$packagePaths=@{'MORROW_SDK_IO_PACKAGE_RUST'=$RustPackage;'MORROW_SDK_IO_PACKAGE_C'=$CPackage;'MORROW_SDK_IO_PACKAGE_CPP'=$CppPackage}
foreach($key in $packagePaths.Keys){$oldPackageEnv[$key]=[Environment]::GetEnvironmentVariable($key,'Process')}
try {
 $selected=@($packagePaths.Values | Where-Object {$_})
 if($selected.Count -ne 0 -and $selected.Count -ne 3){throw 'Supply all three IO packages or none'}
 foreach($key in $packagePaths.Keys){
  $value=if($packagePaths[$key]){(Resolve-Path -LiteralPath $packagePaths[$key]).Path}else{$null}
  Set-ProcessEnv $key $value
 }
 & ./tool/verify_plugin_io_sdk.ps1 -Sysroot $Sysroot -Python $Python
 $env:MORROW_RUST_IO_WASM=(Resolve-Path 'build/sdk-io-guest/wasm32-unknown-unknown/release/morrow_example_io.wasm').Path
 $env:MORROW_SDK_IO_GUEST_C=(Resolve-Path 'build/io-sdk/wasm/c_io.wasm').Path
 $env:MORROW_SDK_IO_GUEST_CPP=(Resolve-Path 'build/io-sdk/wasm/cpp_io.wasm').Path
 Checked cargo @('test','--offline','--locked','--manifest-path','network_node/Cargo.toml','--target-dir','build/sdk-io-network','--features','plugin-adapter','--test','managed_http','--','--nocapture')
 Checked cargo @('test','--offline','--locked','--manifest-path','network_node/Cargo.toml','--target-dir','build/sdk-io-network','--features','plugin-adapter','--test','managed_http','sdk_http_guest::','--','--ignored','--nocapture')
} finally {
 foreach($key in $oldPackageEnv.Keys){Set-ProcessEnv $key $oldPackageEnv[$key]}
 $env:MORROW_RUST_IO_WASM=$oldRust
 $env:MORROW_SDK_IO_GUEST_C=$oldC
 $env:MORROW_SDK_IO_GUEST_CPP=$oldCpp
 Pop-Location
}
