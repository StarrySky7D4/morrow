param(
  [ValidateSet('arm64-v8a','x86_64')][string]$Abi = 'arm64-v8a',
  [string]$RepositoryRoot = (Get-Location).Path,
  [string]$Sdk = 'C:\Program Files\Huawei\DevEco Studio\sdk\default\openharmony'
)
$ErrorActionPreference = 'Stop'
$taskRepoRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$taskHmosRoot = Join-Path $taskRepoRoot 'hmos'
$taskCargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
$taskCapnp = Join-Path $env:LOCALAPPDATA 'Temp/kilo/capnp/capnproto-tools-win32-1.4.0'
$env:Path = "$(Split-Path $taskCargo);$taskCapnp;$env:Path"
$taskTarget = if ($Abi -eq 'arm64-v8a') { 'aarch64-unknown-linux-ohos' } else { 'x86_64-unknown-linux-ohos' }
$taskClangTarget = if ($Abi -eq 'arm64-v8a') { 'aarch64-linux-ohos' } else { 'x86_64-linux-ohos' }
$taskLlvm = Join-Path $Sdk 'native/llvm/bin'
$taskSysroot = (Join-Path $Sdk 'native/sysroot').Replace('\','/')
$taskSuffix = $taskTarget.Replace('-','_')
$env:CC_SHELL_ESCAPED_FLAGS = '1'
[Environment]::SetEnvironmentVariable("CC_$taskSuffix", (Join-Path $taskLlvm 'clang.exe'), 'Process')
[Environment]::SetEnvironmentVariable("AR_$taskSuffix", (Join-Path $taskLlvm 'llvm-ar.exe'), 'Process')
[Environment]::SetEnvironmentVariable("CFLAGS_$taskSuffix", "--target=$taskClangTarget --sysroot=`"$taskSysroot`"", 'Process')
[Environment]::SetEnvironmentVariable("CARGO_TARGET_$($taskSuffix.ToUpper())_LINKER", (Join-Path $taskLlvm 'clang.exe'), 'Process')
$env:CARGO_ENCODED_RUSTFLAGS = "-C$([char]31)link-arg=--target=$taskClangTarget$([char]31)-C$([char]31)link-arg=--sysroot=$taskSysroot"
& $taskCargo build --locked --offline --release --lib --manifest-path (Join-Path $taskHmosRoot 'rust/Cargo.toml') --target $taskTarget --target-dir (Join-Path $taskHmosRoot '.build/rust')
if ($LASTEXITCODE) { throw "Fork checkpoint Rust $taskTarget build failed" }
# Keep previous dev19 and UI integration archives intact.
$taskOutput = Join-Path $taskHmosRoot ".build/editor-input-native/dev22-business-checkpoint/$Abi"
New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
$taskArchive = Join-Path $taskOutput 'libmorrow_hmos.a'
if (Test-Path -LiteralPath $taskArchive) { throw 'Checkpoint archive already exists; reconcile before rebuilding' }
Copy-Item -LiteralPath (Join-Path $taskHmosRoot ".build/rust/$taskTarget/release/libmorrow_hmos.a") -Destination $taskArchive
Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256
