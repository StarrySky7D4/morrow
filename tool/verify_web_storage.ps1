param(
  [string]$Clang = 'clang',
  [string]$Ar = 'llvm-ar',
  [string]$WasmBindgen = '',
  [string]$Dart = 'dart',
  [string]$Node = 'node'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
if (-not $WasmBindgen) { $WasmBindgen = Join-Path $repo 'build/tools/wasm-bindgen/bin/wasm-bindgen.exe' }
$previousCC = $env:CC_wasm32_unknown_unknown
$previousAR = $env:AR_wasm32_unknown_unknown
Push-Location $repo
try {
  $env:CC_wasm32_unknown_unknown = $Clang
  $env:AR_wasm32_unknown_unknown = $Ar
  $version = & $WasmBindgen --version
  if ($LASTEXITCODE -ne 0 -or $version -ne 'wasm-bindgen 0.2.128') { throw 'Install pinned wasm-bindgen-cli 0.2.128 before qualification' }
  & cargo fmt --manifest-path core-web/Cargo.toml --check
  if ($LASTEXITCODE -ne 0) { throw 'Web core format failed' }
  & cargo clippy --locked --manifest-path core-web/Cargo.toml --target-dir build/core-test.8 --target wasm32-unknown-unknown --all-features -- -D warnings
  if ($LASTEXITCODE -ne 0) { throw 'Web core analysis failed' }
  foreach ($profile in @('main', 'fault')) {
    $features = if ($profile -eq 'fault') { @('--features', 'fault-injection') } else { @() }
    & cargo build --locked --manifest-path core-web/Cargo.toml --target-dir build/core-test.8 --target wasm32-unknown-unknown --release @features
    if ($LASTEXITCODE -ne 0) { throw "Web core build failed: $profile" }
    & $WasmBindgen build/core-test.8/wasm32-unknown-unknown/release/morrow_web_core.wasm --target web --out-dir "build/core-test.8/web-store/$profile"
    if ($LASTEXITCODE -ne 0) { throw "Web binding generation failed: $profile" }
  }
  Copy-Item core-web/web/* build/core-test.8/web-store/ -Force
  Push-Location packages/morrow_core_client
  try {
    & $Dart compile js -O2 bin/js_probe.dart -o ../../build/core-test.8/web-store/js_probe.js
    if ($LASTEXITCODE -ne 0) { throw 'Ordinary Dart JavaScript compilation failed' }
  } finally { Pop-Location }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --example protocol_vectors -- build/core-test.8/web-store/vectors
  if ($LASTEXITCODE -ne 0) { throw 'Protocol vectors failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --example web_storage_vectors -- generate build/core-test.8/web-store/vectors/high.morrow
  if ($LASTEXITCODE -ne 0) { throw 'Persistent high revision fixture failed' }
  & $Node tool/test_core_browser.mjs --store
  if ($LASTEXITCODE -ne 0) { throw 'Browser OPFS qualification failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --example web_storage_vectors -- verify build/core-test.8/browser-card.morrow
  if ($LASTEXITCODE -ne 0) { throw 'Native verification of browser export failed' }
} finally {
  $env:CC_wasm32_unknown_unknown = $previousCC
  $env:AR_wasm32_unknown_unknown = $previousAR
  Pop-Location
}
