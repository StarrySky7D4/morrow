$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$previousCc = $env:CC_wasm32_unknown_unknown
$previousAr = $env:AR_wasm32_unknown_unknown
Push-Location $repo
try {
  $bundle = 'build/workbench-host/bundle/workbench.morrowplugin'
  if (!(Test-Path $bundle)) { throw 'Build the native workbench bundle first' }
  $env:CC_wasm32_unknown_unknown = (Get-Command clang).Source
  $env:AR_wasm32_unknown_unknown = (Get-Command llvm-ar).Source
  # Build without qualification/fault features: the Worker exercises production exports.
  & cargo build --locked --release --manifest-path core-web/Cargo.toml --target wasm32-unknown-unknown --target-dir build/web-channel-core
  if ($LASTEXITCODE -ne 0) { throw 'Production browser core compilation failed' }
  & flutter build web --no-pub --release --no-web-resources-cdn --base-href /preview/ --target tool/web_workbench_channel_probe.dart --output build/web-channel-parity
  if ($LASTEXITCODE -ne 0) { throw 'Shared controller browser compilation failed' }
  New-Item -ItemType Directory -Force build/web-channel-parity/workbench/core | Out-Null
  & build/tools/wasm-bindgen/bin/wasm-bindgen.exe build/web-channel-core/wasm32-unknown-unknown/release/morrow_web_core.wasm --target web --out-dir build/web-channel-parity/workbench/core
  if ($LASTEXITCODE -ne 0) { throw 'Production browser core bindings failed' }
  Copy-Item $bundle build/web-channel-parity/workbench/workbench.morrowplugin -Force
  Copy-Item web/workbench/host-worker.mjs,web/workbench/device-identity.mjs build/web-channel-parity/workbench/ -Force
  & node tool/test_core_browser.mjs --channel
  if ($LASTEXITCODE -ne 0) { throw 'Actual browser shared controller qualification failed' }
} finally {
  $env:CC_wasm32_unknown_unknown = $previousCc
  $env:AR_wasm32_unknown_unknown = $previousAr
  Pop-Location
}
