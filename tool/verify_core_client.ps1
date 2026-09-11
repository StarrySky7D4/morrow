param(
  [switch]$Web,
  [string]$Python = 'python'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
  & $Python -X utf8 tool/generate_core_client.py --check
  if ($LASTEXITCODE -ne 0) { throw 'Generated bindings differ from schema' }
  & cargo build --locked --manifest-path core/Cargo.toml --target-dir build/core-test.5 --lib
  if ($LASTEXITCODE -ne 0) { throw 'Native core build failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.5 --example protocol_vectors -- build/core-test.5/vectors
  if ($LASTEXITCODE -ne 0) { throw 'Protocol vectors failed' }
  $library = if ($IsWindows) { 'morrow_core.dll' } elseif ($IsMacOS) { 'libmorrow_core.dylib' } else { 'libmorrow_core.so' }
  $nativeLibrary = Join-Path $repo "build/core-test.5/debug/$library"
  # The Flutter frontend avoids Dart 3.12's Windows perf-socket shutdown race.
  & flutter analyze --no-pub packages/morrow_core_client
  if ($LASTEXITCODE -ne 0) { throw 'Client analysis failed' }
  Push-Location packages/morrow_core_client
  try {
    & dart test
    if ($LASTEXITCODE -ne 0) { throw 'Client tests failed' }
    & dart run bin/native_probe.dart $nativeLibrary
    if ($LASTEXITCODE -ne 0) { throw 'Native client probe failed' }
    if ($Web) {
      New-Item -ItemType Directory -Force ../../build/core-test.5/web | Out-Null
      & dart compile wasm bin/web_probe.dart -o ../../build/core-test.5/web/probe.wasm
      if ($LASTEXITCODE -ne 0) { throw 'Dart/Wasm probe compile failed' }
    }
  } finally { Pop-Location }
  if ($Web) {
    & cargo build --locked --manifest-path core/Cargo.toml --target-dir build/core-test.5 --target wasm32-unknown-unknown --release --lib
    if ($LASTEXITCODE -ne 0) { throw 'Rust/Wasm build failed' }
    Copy-Item -LiteralPath build/core-test.5/wasm32-unknown-unknown/release/morrow_core.wasm -Destination build/core-test.5/web/morrow_core.wasm -Force
    Copy-Item -LiteralPath packages/morrow_core_client/bin/web_probe.html -Destination build/core-test.5/web/index.html -Force
    Copy-Item -LiteralPath packages/morrow_core_client/bin/core_worker.mjs -Destination build/core-test.5/web/core_worker.mjs -Force
    New-Item -ItemType Directory -Force build/core-test.5/web/vectors | Out-Null
    Get-ChildItem -LiteralPath build/core-test.5/vectors -File -Filter '*.capnp' | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $repo "build/core-test.5/web/vectors/$($_.Name)") -Force }
    & node tool/test_core_browser.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Browser interoperability failed' }
  }
} finally { Pop-Location }
