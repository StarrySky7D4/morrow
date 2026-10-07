param(
  [string]$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path,
  [switch]$SourceOnly,
  [switch]$ProductionArchives,
  [switch]$IncludeFlutterReferences,
  [switch]$IncludeExternalDependencies
)
$ErrorActionPreference = 'Stop'
$taskRepoRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$taskManifestPath = Join-Path $taskRepoRoot 'hmos/reports/ui-source/v19/field-native-inputs.json'
$taskManifest = Get-Content -LiteralPath $taskManifestPath -Raw | ConvertFrom-Json
if ($taskManifest.candidate_release -ne 'dev19') { throw 'Expected dev19 native input manifest' }

function Test-InputFile($taskRecord, [string]$taskBaseRoot = $taskRepoRoot) {
  $taskPath = [IO.Path]::GetFullPath((Join-Path $taskBaseRoot $taskRecord.path))
  $taskBase = [IO.Path]::GetFullPath($taskBaseRoot).TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar
  if (-not $taskPath.StartsWith($taskBase, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Input escapes its declared root: $($taskRecord.path)"
  }
  $taskFile = Get-Item -LiteralPath $taskPath
  if ($taskFile.PSIsContainer -or $taskFile.Length -ne $taskRecord.bytes) { throw "Input size drift: $($taskRecord.path)" }
  if ((Get-FileHash -LiteralPath $taskPath -Algorithm SHA256).Hash -ne $taskRecord.sha256) {
    throw "Input hash drift: $($taskRecord.path)"
  }
}

$taskInventoryPaths = @($taskManifest.repository_source_inventory.path)
if (($taskInventoryPaths | Sort-Object -Unique).Count -ne $taskInventoryPaths.Count) { throw 'Duplicate source inventory paths' }
# Native Cargo builds use hmos/rust/Cargo.lock. Incidental standalone dependency
# lockfiles under frozen shared packages are not inputs to that Cargo graph.
$taskDiscovered = @(Get-ChildItem (Join-Path $taskRepoRoot 'hmos/rust'),(Join-Path $taskRepoRoot 'hmos/shared') -File -Recurse |
  Where-Object { $_.FullName -notmatch '[\\/](target|\.build|\.git)[\\/]' -and
    ($_.Extension -in '.rs','.proto','.capnp' -or $_.Name -eq 'Cargo.toml') } |
  ForEach-Object { $_.FullName.Substring($taskRepoRoot.Length + 1).Replace('\','/') })
$taskMissing = @($taskDiscovered | Where-Object { $_ -notin $taskInventoryPaths })
if ($taskMissing.Count) { throw "Native source files missing from inventory: $($taskMissing -join ', ')" }
foreach ($taskRecord in $taskManifest.repository_source_inventory) { Test-InputFile $taskRecord }

if (-not $SourceOnly) {
  foreach ($taskRecord in @($taskManifest.archives) + @($taskManifest.preserved_dev18_archives)) { Test-InputFile $taskRecord }
}
if ($ProductionArchives) {
  foreach ($taskRecord in $taskManifest.archives) {
    $taskAbi = Split-Path (Split-Path $taskRecord.path -Parent) -Leaf
    Test-InputFile ([pscustomobject]@{
      path = "hmos/entry/src/main/cpp/rust/$taskAbi/libmorrow_hmos.a"
      bytes = $taskRecord.bytes; sha256 = $taskRecord.sha256
    })
  }
}
if ($IncludeFlutterReferences) {
  foreach ($taskRecord in $taskManifest.flutter_reference_files) { Test-InputFile $taskRecord }
  foreach ($taskRecord in $taskManifest.flutter_external_package.files) {
    Test-InputFile $taskRecord (Join-Path $env:LOCALAPPDATA 'Pub/Cache/hosted/pub.dev/characters-1.4.1')
  }
}
if ($IncludeExternalDependencies) {
  $taskDependencyRoots = @(Get-ChildItem (Join-Path $env:USERPROFILE '.cargo/registry/src') -Directory |
    ForEach-Object { Join-Path $_.FullName 'unicode-segmentation-1.12.0' } | Where-Object { Test-Path -LiteralPath $_ })
  if ($taskDependencyRoots.Count -ne 1) { throw 'Expected one pinned Unicode 16 dependency source root' }
  foreach ($taskRecord in $taskManifest.external_unicode_dependency.files) { Test-InputFile $taskRecord $taskDependencyRoots[0] }
}
[pscustomobject]@{
  status = 'PASS'; candidate_release = 'dev19'; repository_sources = $taskManifest.repository_source_inventory.Count
  candidate_archives_checked = $(if ($SourceOnly) { 0 } else { $taskManifest.archives.Count })
  preserved_dev18_archives_checked = $(if ($SourceOnly) { 0 } else { $taskManifest.preserved_dev18_archives.Count })
  production_archives_checked = [bool]$ProductionArchives
  flutter_references_checked = [bool]$IncludeFlutterReferences
  external_dependencies_checked = [bool]$IncludeExternalDependencies
} | ConvertTo-Json
