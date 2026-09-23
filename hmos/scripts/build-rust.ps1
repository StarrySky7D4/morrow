param([ValidateSet('arm64-v8a','x86_64')][string]$Abi='arm64-v8a', [string]$Sdk='C:\Program Files\Huawei\DevEco Studio\sdk\default\openharmony', [switch]$Test, [switch]$Runner)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$cargo=Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
$capnp=Join-Path $env:LOCALAPPDATA 'Temp/kilo/capnp/capnproto-tools-win32-1.4.0'
$env:Path="$(Split-Path $cargo);$capnp;$env:Path"
if ($Test) {
  & $cargo test --locked --offline --manifest-path "$root/rust/Cargo.toml" --target-dir "$root/.build/rust" --lib
  if ($LASTEXITCODE) {throw 'Host Rust tests failed'}
  exit 0
}
$target=if($Abi -eq 'arm64-v8a'){'aarch64-unknown-linux-ohos'}else{'x86_64-unknown-linux-ohos'}
$clangTarget=if($Abi -eq 'arm64-v8a'){'aarch64-linux-ohos'}else{'x86_64-linux-ohos'}
$llvm=Join-Path $Sdk 'native/llvm/bin'
$sysroot=(Join-Path $Sdk 'native/sysroot').Replace('\','/')
$env:CC_SHELL_ESCAPED_FLAGS='1'
$suffix=$target.Replace('-','_')
[Environment]::SetEnvironmentVariable("CC_$suffix",(Join-Path $llvm 'clang.exe'),'Process')
[Environment]::SetEnvironmentVariable("AR_$suffix",(Join-Path $llvm 'llvm-ar.exe'),'Process')
[Environment]::SetEnvironmentVariable("CFLAGS_$suffix","--target=$clangTarget --sysroot=`"$sysroot`"",'Process')
[Environment]::SetEnvironmentVariable("CARGO_TARGET_$($suffix.ToUpper())_LINKER",(Join-Path $llvm 'clang.exe'),'Process')
$env:CARGO_ENCODED_RUSTFLAGS="-C$([char]31)link-arg=--target=$clangTarget$([char]31)-C$([char]31)link-arg=--sysroot=$sysroot"
& $cargo build --locked --offline --release --lib --manifest-path "$root/rust/Cargo.toml" --target $target --target-dir "$root/.build/rust"
if ($LASTEXITCODE) {throw "Rust $target build failed"}
if($Runner) {
  & $cargo build --locked --offline --release --bin hmos-self-check --manifest-path "$root/rust/Cargo.toml" --target $target --target-dir "$root/.build/rust"
  if($LASTEXITCODE) {throw 'OHOS self-check executable build failed'}
}
$out=Join-Path $root "entry/src/main/cpp/rust/$Abi"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item -LiteralPath "$root/.build/rust/$target/release/libmorrow_hmos.a" -Destination "$out/libmorrow_hmos.a"
Get-FileHash -LiteralPath "$out/libmorrow_hmos.a" -Algorithm SHA256
