param([string]$Python='python')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){ & $Program @Arguments; if($LASTEXITCODE -ne 0){throw "Plugin runtime verification failed: $Program"} }
Push-Location $repo
try {
 Checked $Python @('tool/sync_plugin_sdk_contracts.py','--check')
 foreach($manifest in @('plugin_runtime/Cargo.toml','sdk/rust/Cargo.toml','sdk/examples/rust-rename/Cargo.toml')) {
  Checked cargo @('fmt','--manifest-path',$manifest,'--check')
 }
 Checked cargo @('clippy','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/plugin-runtime','--all-targets','--','-D','warnings')
 Checked cargo @('test','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/plugin-runtime')
 Checked cargo @('clippy','--locked','--manifest-path','sdk/rust/Cargo.toml','--features','wasm-guest','--target','wasm32-unknown-unknown','--target-dir','build/plugin-sdk/rust','--','-D','warnings')
 Checked cargo @('test','--locked','--manifest-path','sdk/rust/Cargo.toml','--target-dir','build/plugin-sdk/rust')
 Checked cargo @('clippy','--locked','--manifest-path','sdk/examples/rust-rename/Cargo.toml','--target','wasm32-unknown-unknown','--target-dir','build/plugin-guest','--','-D','warnings')
 Checked cargo @('build','--locked','--manifest-path','sdk/examples/rust-rename/Cargo.toml','--target','wasm32-unknown-unknown','--release','--target-dir','build/plugin-guest')
 Checked cargo @('run','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir','build/plugin-runtime','--example','qualify','--','build/plugin-guest/wasm32-unknown-unknown/release/morrow_example_rename.wasm')
 Get-FileHash -Algorithm SHA256 -LiteralPath 'build/plugin-guest/wasm32-unknown-unknown/release/morrow_example_rename.wasm'
} finally {Pop-Location}
