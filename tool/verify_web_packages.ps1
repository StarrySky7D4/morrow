$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$previousCc = $env:CC_wasm32_unknown_unknown
$previousAr = $env:AR_wasm32_unknown_unknown
Push-Location $repo
try {
  if (!(Test-Path build/workbench-host/bundle/workbench.morrowplugin)) {
    throw 'Build the native bundle first with tool/build_workbench_bundle.ps1'
  }
  $env:CC_wasm32_unknown_unknown = (Get-Command clang).Source
  $env:AR_wasm32_unknown_unknown = (Get-Command llvm-ar).Source
  & cargo build --locked --manifest-path core-web/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/core-test.10
  if ($LASTEXITCODE -ne 0) { throw 'Web package runtime compilation failed' }
  & build/tools/wasm-bindgen/bin/wasm-bindgen.exe build/core-test.10/wasm32-unknown-unknown/release/morrow_web_core.wasm --target web --out-dir build/web-package-parity/main
  if ($LASTEXITCODE -ne 0) { throw 'Web bindings failed' }
  & cargo run --locked --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --example web_package_vectors -- build/workbench-host/bundle/workbench.morrowplugin build/web-package-parity/vectors
  if ($LASTEXITCODE -ne 0) { throw 'Native package parity vector generation failed' }
  Copy-Item core-web/web/package-worker.mjs build/web-package-parity/package-worker.mjs -Force
  Copy-Item core-web/web/package-dependencies.mjs build/web-package-parity/package-dependencies.mjs -Force
  Copy-Item core-web/web/package-index.html build/web-package-parity/index.html -Force
  & node tool/test_core_browser.mjs --packages
  if ($LASTEXITCODE -ne 0) { throw 'Actual browser package parity failed' }
} finally {
  $env:CC_wasm32_unknown_unknown = $previousCc
  $env:AR_wasm32_unknown_unknown = $previousAr
  Pop-Location
}
