param([string]$Source = (Join-Path $PSScriptRoot '../../build/io-safety-refactor'))
$ErrorActionPreference = 'Stop'
$sourceRoot = (Resolve-Path -LiteralPath $Source).Path
$destination = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../shared'))
if (Test-Path -LiteralPath (Join-Path $destination 'reference.json')) {
  throw 'Snapshot already exists. Review upstream changes before replacing the pinned reference.'
}
$paths = @('core/', 'plugins/workbench/', 'sdk/rust/')
$files = & git -C $sourceRoot ls-files --cached --others --exclude-standard -- core plugins/workbench sdk/rust
if ($LASTEXITCODE) { throw 'Cannot enumerate reference source.' }
$records = @()
foreach ($relative in ($files | Sort-Object -Unique)) {
  if ($relative -notmatch '\.(rs|proto|capnp|toml|lock|h|md|txt)$') { continue }
  $original = Join-Path $sourceRoot $relative
  if (!(Test-Path -LiteralPath $original -PathType Leaf)) { continue }
  $before = (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
  $target = Join-Path $destination $relative
  New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
  Copy-Item -LiteralPath $original -Destination $target
  $after = (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
  $copied = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
  if ($before -ne $after -or $before -ne $copied) { throw "Source changed during snapshot: $relative" }
  $records += [ordered]@{ path=$relative; sha256=$copied }
}
# Recheck the entire set: the upstream task may still be editing it.
foreach ($record in $records) {
  if ((Get-FileHash -LiteralPath (Join-Path $sourceRoot $record.path)).Hash -ne $record.sha256) {
    throw "Source changed during snapshot: $($record.path)"
  }
}
Copy-Item -LiteralPath (Join-Path $sourceRoot 'LICENSE') -Destination (Join-Path $destination 'LICENSE')
[ordered]@{
  sourceThread='01a085bd-7a94-7f93-8a1f-1ecf417f5ee3'
  sourceRoot=$sourceRoot
  head=(& git -C $sourceRoot rev-parse HEAD)
  capturedUtc=[DateTime]::UtcNow.ToString('o')
  includesUncommittedSource=$true
  files=$records
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $destination 'reference.json') -Encoding utf8
Write-Output "Pinned $($records.Count) source files."
