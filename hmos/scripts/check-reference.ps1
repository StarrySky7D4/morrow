param([string]$Source = (Join-Path $PSScriptRoot '../../build/io-safety-refactor'))
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$baseline=Get-Content -Raw -LiteralPath "$root/shared/reference.json" | ConvertFrom-Json
$changes=@()
foreach($file in $baseline.files) {
  $upstream=Join-Path $Source $file.path
  $snapshot=Join-Path "$root/shared" $file.path
  if(!(Test-Path -LiteralPath $snapshot) -or (Get-FileHash -LiteralPath $snapshot).Hash -ne $file.sha256) {
    throw "Pinned source modified: $($file.path)"
  }
  if(!(Test-Path -LiteralPath $upstream)) {$changes += "deleted: $($file.path)"}
  elseif((Get-FileHash -LiteralPath $upstream).Hash -ne $file.sha256) {$changes += "changed: $($file.path)"}
}
$known=@{}; foreach($file in $baseline.files) {$known[$file.path]=$true}
$current=& git -C $Source ls-files --cached --others --exclude-standard -- core plugins/workbench sdk/rust
if($LASTEXITCODE) {throw 'Cannot inspect upstream reference'}
foreach($file in ($current | Sort-Object -Unique)) {
  if($file -match '\.(rs|proto|capnp|toml|lock|h|md|txt)$' -and !$known.ContainsKey($file)) {$changes += "added: $file"}
}
$result=[ordered]@{checkedUtc=[DateTime]::UtcNow.ToString('o');sourceThread=$baseline.sourceThread;head=(& git -C $Source rev-parse HEAD);changes=$changes;matches=($changes.Count -eq 0)}
$result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath "$root/reports/reference-drift.json" -Encoding utf8
$result | ConvertTo-Json -Depth 4
