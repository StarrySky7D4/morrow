param([switch]$SkipWasm)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $root
try {
  function Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {throw "$Program failed: $LASTEXITCODE"}
  }
  Checked cargo @('test','--offline','--locked','--manifest-path','audit/Cargo.toml','--target-dir','build/audit')
  Checked cargo @('test','--offline','--locked','--manifest-path','audit/Cargo.toml','--target-dir','build/audit','--features','fault-injection','--test','archive','--','--nocapture')
  Checked cargo @('test','--offline','--locked','--manifest-path','audit/Cargo.toml','--target-dir','build/audit','--features','fault-injection','--test','keys','--','--nocapture')
  Checked cargo @('test','--offline','--locked','--manifest-path','audit/Cargo.toml','--target-dir','build/audit','--features','fault-injection','--test','session','--','--nocapture')
  Checked cargo @('clippy','--offline','--locked','--manifest-path','audit/Cargo.toml','--target-dir','build/audit','--all-targets','--features','fault-injection','--','-D','warnings')
  Checked cargo @('build','--offline','--locked','--release','--manifest-path','audit/Cargo.toml','--target-dir','build/audit','--bins','--example','qualify','--example','qualify_keys','--example','qualify_session')
  $evidence = Join-Path $root ('build/audit/qualification-'+[guid]::NewGuid().ToString('N'))
  Checked build/audit/release/examples/qualify.exe @($evidence)
  $key = (Get-Content -LiteralPath (Join-Path $evidence 'test-public-key.txt') -Raw).Trim()
  $one = Join-Path $evidence '0001.maudit'
  $two = Join-Path $evidence '0002.maudit'
  $checkpoint = Join-Path $evidence 'independent-checkpoint.maudit'
  Checked build/audit/release/morrow-audit-check.exe @($key,'synthetic-audit','--checkpoint',$checkpoint,$one,$two)
  $coreDb = Join-Path $evidence 'synthetic.db'
  $coreBefore = (Get-FileHash -LiteralPath $coreDb -Algorithm SHA256).Hash
  Checked build/audit/release/morrow-audit-check.exe @($key,'synthetic-audit','--checkpoint',$checkpoint,'--core-store',$coreDb)
  if ((Get-FileHash -LiteralPath $coreDb -Algorithm SHA256).Hash -ne $coreBefore) {throw 'Read-only core verification changed the database'}
  $archive = Join-Path $evidence 'archive.db'
  $before = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
  Checked build/audit/release/morrow-audit-check.exe @($key,'synthetic-audit','--checkpoint',$checkpoint,'--archive',$archive)
  if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $before) {throw 'Read-only archive verification changed the database'}
  & build/audit/release/morrow-audit-check.exe $key synthetic-audit --checkpoint $checkpoint $one 2>&1 | Out-File (Join-Path $evidence 'rollback-rejection.txt')
  if ($LASTEXITCODE -eq 0) {throw 'Verifier accepted history behind its pinned checkpoint'}
  & build/audit/release/morrow-audit-check.exe $key synthetic-audit --checkpoint $checkpoint --archive (Join-Path $evidence 'truncated.db') 2>&1 | Out-File (Join-Path $evidence 'archive-rollback-rejection.txt')
  if ($LASTEXITCODE -eq 0) {throw 'Archive verifier accepted a truncated archive behind its checkpoint'}
  if (-not $SkipWasm) {
    Checked cargo @('check','--offline','--locked','--manifest-path','audit/Cargo.toml','--target-dir','build/audit','--target','wasm32-unknown-unknown','--lib')
  }
  $protectedEvidence = Join-Path $root ('build/audit/protected-key-'+[guid]::NewGuid().ToString('N'))
  Checked build/audit/release/examples/qualify_keys.exe @($protectedEvidence)
  $protectedKey = Join-Path $protectedEvidence 'protected.morrowkey'
  $protectedDb = Join-Path $protectedEvidence 'synthetic.db'
  Checked build/audit/release/morrow-audit-seal.exe @($protectedKey,$protectedDb)
  Checked build/audit/release/morrow-audit-seal.exe @($protectedKey,$protectedDb)
  $publicKey = (Get-Content -LiteralPath (Join-Path $protectedEvidence 'public-key.txt') -Raw).Trim()
  $logId = (Get-Content -LiteralPath (Join-Path $protectedEvidence 'log-id.txt') -Raw).Trim()
  Checked build/audit/release/morrow-audit-check.exe @($publicKey,$logId,'--core-store',$protectedDb)
  Write-Output "Protected key qualification passed: $protectedEvidence"
  Write-Output "Audit qualification passed: $evidence"
} finally {Pop-Location}
