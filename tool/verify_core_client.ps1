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
  & cargo build --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --lib
  if ($LASTEXITCODE -ne 0) { throw 'Native core build failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --example protocol_vectors -- build/core-test.8/vectors
  if ($LASTEXITCODE -ne 0) { throw 'Protocol vectors failed' }
  & cargo build --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store
  if ($LASTEXITCODE -ne 0) { throw 'Native host fixture CLI build failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --example web_storage_vectors -- generate build/core-test.8/vectors/high.morrow
  if ($LASTEXITCODE -ne 0) { throw 'High revision fixture failed' }
  $library = if ($IsWindows) { 'morrow_core.dll' } elseif ($IsMacOS) { 'libmorrow_core.dylib' } else { 'libmorrow_core.so' }
  $nativeLibrary = Join-Path $repo "build/core-test.8/debug/$library"
  # The Flutter frontend avoids Dart 3.12's Windows perf-socket shutdown race.
  & flutter analyze --no-pub packages/morrow_core_client
  if ($LASTEXITCODE -ne 0) { throw 'Client analysis failed' }
  Push-Location packages/morrow_core_client
  try {
    & dart test
    if ($LASTEXITCODE -ne 0) { throw 'Client tests failed' }
    & dart run bin/native_probe.dart $nativeLibrary
    if ($LASTEXITCODE -ne 0) { throw 'Native client probe failed' }
    $storeCli = Join-Path $repo $(if ($IsWindows) { 'build/core-test.8/debug/morrow-core-store.exe' } else { 'build/core-test.8/debug/morrow-core-store' })
    & dart run bin/native_host_probe.dart $nativeLibrary $storeCli (Join-Path $repo 'build/core-test.8/vectors/high.morrow') (Join-Path $repo 'build/core-test.8')
    if ($LASTEXITCODE -ne 0) { throw 'Native host integration failed' }
    if ($Web) {
      New-Item -ItemType Directory -Force ../../build/core-test.8/web | Out-Null
      & dart compile wasm bin/web_probe.dart -o ../../build/core-test.8/web/probe.wasm
      if ($LASTEXITCODE -ne 0) { throw 'Dart/Wasm probe compile failed' }
    }
  } finally { Pop-Location }
  if ($Web) {
    & cargo build --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --target wasm32-unknown-unknown --release --lib
    if ($LASTEXITCODE -ne 0) { throw 'Rust/Wasm build failed' }
    Copy-Item -LiteralPath build/core-test.8/wasm32-unknown-unknown/release/morrow_core.wasm -Destination build/core-test.8/web/morrow_core.wasm -Force
    Copy-Item -LiteralPath packages/morrow_core_client/bin/web_probe.html -Destination build/core-test.8/web/index.html -Force
    Copy-Item -LiteralPath packages/morrow_core_client/bin/core_worker.mjs -Destination build/core-test.8/web/core_worker.mjs -Force
    New-Item -ItemType Directory -Force build/core-test.8/web/vectors | Out-Null
    Get-ChildItem -LiteralPath build/core-test.8/vectors -File -Filter '*.capnp' | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $repo "build/core-test.8/web/vectors/$($_.Name)") -Force }
    & node tool/test_core_browser.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Browser interoperability failed' }
  }
} finally { Pop-Location }
