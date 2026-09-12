$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
  & cargo build --locked --manifest-path plugins/workbench/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/first-party-plugins
  if ($LASTEXITCODE -ne 0) {throw 'Rust workbench guest build failed'}
  & cargo build --locked --manifest-path workbench_host/Cargo.toml --release --target-dir build/workbench-host
  if ($LASTEXITCODE -ne 0) {throw 'Rust workbench host build failed'}
  New-Item -ItemType Directory -Force -Path build/workbench-host/bundle | Out-Null
  & build/workbench-host/release/package.exe build/first-party-plugins/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm build/workbench-host/bundle/workbench.morrowplugin
  if ($LASTEXITCODE -ne 0) {throw 'Rust workbench package verification failed'}
} finally {Pop-Location}
