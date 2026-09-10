param([switch]$Web)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
  & cargo fmt --manifest-path core/Cargo.toml --check
  if ($LASTEXITCODE -ne 0) { throw 'Rust format check failed' }
  & cargo clippy --locked --manifest-path core/Cargo.toml --target-dir build/core-test.3 --all-targets -- -D warnings
  if ($LASTEXITCODE -ne 0) { throw 'Rust analysis failed' }
  & cargo test --locked --manifest-path core/Cargo.toml --target-dir build/core-test.3
  if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
  & cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.3 --bin morrow-core-check -- self-check
  if ($LASTEXITCODE -ne 0) { throw 'Core self-check failed' }
  if ($Web) {
    & cargo check --locked --manifest-path core/Cargo.toml --target-dir build/core-test.3 --target wasm32-unknown-unknown --lib
    if ($LASTEXITCODE -ne 0) { throw 'Web core compile check failed' }
  }
} finally { Pop-Location }
