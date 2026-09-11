param([switch]$Web)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
  & cargo fmt --manifest-path core/Cargo.toml --check
  if ($LASTEXITCODE -ne 0) { throw 'Rust format check failed' }
  & cargo clippy --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --all-targets -- -D warnings
  if ($LASTEXITCODE -ne 0) { throw 'Rust analysis failed' }
  & cargo test --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8
  if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
  & cargo test --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --features fault-injection --test storage_crash --test attachment_crash
  if ($LASTEXITCODE -ne 0) { throw 'Storage crash recovery failed' }
  & cargo build --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store
  if ($LASTEXITCODE -ne 0) { throw 'Default storage CLI build failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-check -- self-check
  if ($LASTEXITCODE -ne 0) { throw 'Core self-check failed' }
  if ($Web) {
    & cargo check --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --target wasm32-unknown-unknown --lib
    if ($LASTEXITCODE -ne 0) { throw 'Web core compile check failed' }
  }
} finally { Pop-Location }
