$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$previousCc = $env:CC_wasm32_unknown_unknown
$previousAr = $env:AR_wasm32_unknown_unknown
Push-Location $repo
try {
  $bundle = 'build/workbench-host/bundle/workbench.morrowplugin'
  if (!(Test-Path $bundle)) { throw 'Build the native bundle with tool/build_workbench_bundle.ps1 first' }
  New-Item -ItemType Directory -Force build/web-workbench-parity | Out-Null
  & cargo run --locked --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --example web_workbench_probe -- $bundle build/web-workbench-parity/fixture-public.bin
  if ($LASTEXITCODE -ne 0) { throw 'Native workbench protocol qualification failed' }
  $env:CC_wasm32_unknown_unknown = (Get-Command clang).Source
  $env:AR_wasm32_unknown_unknown = (Get-Command llvm-ar).Source
  & cargo build --locked --release --manifest-path core-web/Cargo.toml --target wasm32-unknown-unknown --features workbench-qualification --target-dir build/core-test.10
  if ($LASTEXITCODE -ne 0) { throw 'Browser workbench compilation failed' }
  & build/tools/wasm-bindgen/bin/wasm-bindgen.exe build/core-test.10/wasm32-unknown-unknown/release/morrow_web_core.wasm --target web --out-dir build/web-workbench-parity/main
  if ($LASTEXITCODE -ne 0) { throw 'Browser bindings failed' }
  Copy-Item $bundle build/web-workbench-parity/workbench.morrowplugin -Force
  Copy-Item core-web/web/workbench-worker.mjs build/web-workbench-parity/workbench-worker.mjs -Force
  Copy-Item core-web/web/workbench-index.html build/web-workbench-parity/index.html -Force
  & node tool/test_core_browser.mjs --workbench
  if ($LASTEXITCODE -ne 0) { throw 'Actual browser workbench qualification failed' }
} finally {
  $env:CC_wasm32_unknown_unknown = $previousCc
  $env:AR_wasm32_unknown_unknown = $previousAr
  Pop-Location
}
