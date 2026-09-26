param(
  [string]$RetainedPackage = $env:MORROW_RETAINED_WORKBENCH_PACKAGE,
  [string]$RetainedSha256 = $env:MORROW_RETAINED_WORKBENCH_SHA256
)
$ErrorActionPreference = 'Stop'
function Get-BytesSha256([byte[]]$Bytes) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
}
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
  # Explicit release-byte reuse: never silently replace a same-version package.
  # Read once so the verified bytes are the exact bytes installed after building.
  $retainedBytes = $null
  if ($RetainedPackage -or $RetainedSha256) {
    if (-not $RetainedPackage -or $RetainedSha256 -notmatch '^[a-fA-F0-9]{64}$') {
      throw 'Retained workbench package requires a path and exact SHA-256'
    }
    $retainedBytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $RetainedPackage).Path)
    $actual = Get-BytesSha256 $retainedBytes
    if ($actual -ne $RetainedSha256.ToLowerInvariant()) { throw 'Retained workbench package SHA-256 mismatch' }
    $existing = 'build/workbench-host/bundle/workbench.morrowplugin'
    if ((Test-Path -LiteralPath $existing) -and (Get-BytesSha256 ([IO.File]::ReadAllBytes((Join-Path $projectRoot $existing)))) -ne $actual) {
      throw 'Existing bundle differs from retained package; use a separate build directory'
    }
  }
  & cargo build --locked --manifest-path plugins/workbench/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/first-party-plugins
  if ($LASTEXITCODE -ne 0) {throw 'Rust workbench guest build failed'}
  & cargo build --locked --manifest-path workbench_host/Cargo.toml --release --target-dir build/workbench-host
  if ($LASTEXITCODE -ne 0) {throw 'Rust workbench host build failed'}
  New-Item -ItemType Directory -Force -Path build/workbench-host/bundle | Out-Null
  if ($null -ne $retainedBytes) {
    [IO.File]::WriteAllBytes((Join-Path $projectRoot 'build/workbench-host/bundle/workbench.morrowplugin'), $retainedBytes)
    Write-Output "Retained workbench package SHA-256 verified: $actual (new guest compiled separately)"
  } else {
    & build/workbench-host/release/package.exe build/first-party-plugins/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm build/workbench-host/bundle/workbench.morrowplugin
    if ($LASTEXITCODE -ne 0) {throw 'Rust workbench package verification failed'}
  }
} finally {Pop-Location}
