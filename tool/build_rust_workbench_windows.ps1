#requires -Version 7.0
param([switch]$SkipVerify, [switch]$RefreshArtifact, [string]$OutputRoot = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Program failed: $LASTEXITCODE" }
}
if ($RefreshArtifact) { throw 'Preview artifacts are immutable. Use a new version or a fresh output directory; existing outputs are retained.' }
$versionMatch = [regex]::Match((Get-Content pubspec.yaml -Raw),'(?m)^version: ([^+\r\n]+)')
if (-not $versionMatch.Success) { throw 'Missing pubspec version' }
$version = $versionMatch.Groups[1].Value
if ($version -notmatch '^[0-9A-Za-z][0-9A-Za-z.-]*$') { throw 'Invalid version for artifact names' }
$customRoot = -not [string]::IsNullOrWhiteSpace($OutputRoot)
if ($customRoot) {
  if ([IO.Path]::IsPathRooted($OutputRoot)) {
    $outputDirectory = [IO.Path]::GetFullPath($OutputRoot)
  } else {
    $outputDirectory = [IO.Path]::GetFullPath((Join-Path $projectRoot $OutputRoot))
  }
  if (Test-Path -LiteralPath $outputDirectory) { throw "Output root must be new: $outputDirectory" }
} else {
  $outputDirectory = Join-Path $projectRoot 'dist'
}
# A new in-repository output must be ignored by Git, so its own source archive
# and staged files cannot enter the working-tree snapshot mid-build.
$relativeOutput = [IO.Path]::GetRelativePath($projectRoot, $outputDirectory)
if (-not $relativeOutput.StartsWith('..' + [IO.Path]::DirectorySeparatorChar) -and
    $relativeOutput -ne '..' -and -not [IO.Path]::IsPathRooted($relativeOutput)) {
  $relativeGitPath = $relativeOutput.Replace('\', '/')
  & git check-ignore -q --no-index -- $relativeGitPath
  if ($LASTEXITCODE -ne 0) { throw "In-repository output root must be Git-ignored: $outputDirectory" }
}
$ancestor = $outputDirectory
while ($ancestor) {
  if (Test-Path -LiteralPath $ancestor) {
    $item = Get-Item -LiteralPath $ancestor -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      throw "Output path contains a reparse point: $ancestor"
    }
  }
  $parent = Split-Path -Path $ancestor -Parent
  if (-not $parent -or $parent -eq $ancestor) { break }
  $ancestor = $parent
}
$sourceArchive = Join-Path $outputDirectory "morrow-$version-source.zip"
$destination = Join-Path $outputDirectory "morrow-$version-rust-workbench-windows"
$archive = "$destination.zip"
foreach ($path in @($sourceArchive, "$sourceArchive.sha256", $destination, $archive, "$archive.sha256")) {
  if (Test-Path -LiteralPath $path) { throw "Preserving existing artifact before build: $path" }
}
Checked python @('-X','utf8','tool/build_i18n.py','--check')
Checked python @('-X','utf8','tool/package_preview_source.py',$sourceArchive)
$sourceCommit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') { throw 'Cannot identify source commit' }
$sourceHash = (Get-FileHash -LiteralPath $sourceArchive -Algorithm SHA256).Hash.ToLowerInvariant()
Checked python @('-X','utf8','tool/generate_workbench_client.py','--check')
Checked flutter @('build','windows','--release','--no-pub','--target','lib/main.dart')
if (-not $SkipVerify) {
  $env:MORROW_WORKBENCH_HOST = (Resolve-Path build/workbench-host/release/morrow-workbench-host.exe).Path
  $env:MORROW_WORKBENCH_PACKAGE = (Resolve-Path build/workbench-host/bundle/workbench.morrowplugin).Path
  Checked flutter @('test','--no-pub','test/rust_workbench_integration_test.dart')
}
Checked python @('-X','utf8','tool/package_preview_source.py',$sourceArchive,'--check')
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$releaseRoot = (Resolve-Path build/windows/x64/runner/Release).Path
foreach ($file in Get-ChildItem -LiteralPath $releaseRoot -Recurse -File) {
  $relative = [IO.Path]::GetRelativePath($releaseRoot, $file.FullName)
  # Only the two freshly built first-party packages are shipped; old cached
  # plugins in a reused Flutter Release directory never enter this preview.
  if ($relative.StartsWith('plugins' + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { continue }
  $targetFile = Join-Path $destination $relative
  New-Item -ItemType Directory -Force -Path (Split-Path $targetFile) | Out-Null
  Copy-Item -LiteralPath $file.FullName -Destination $targetFile -Force
}
# Bundle the matching installed MSVC runtime for a standalone preview.
$crt = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Redist\MSVC\14.44.35112\x64\Microsoft.VC143.CRT'
if (Test-Path -LiteralPath $crt) { Copy-Item -Path "$crt/*.dll" -Destination $destination -Force }
Copy-Item -LiteralPath build/workbench-host/release/morrow-workbench-host.exe -Destination $destination
New-Item -ItemType Directory -Force -Path (Join-Path $destination plugins) | Out-Null
Copy-Item -LiteralPath build/workbench-host/bundle/workbench.morrowplugin -Destination (Join-Path $destination plugins/workbench.morrowplugin)
$theme = Get-Content plugins/mid_autumn/theme.json -Raw -Encoding UTF8 | ConvertFrom-Json
if ($theme.version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Invalid Mid-Autumn theme version' }
$themeName = "morrow-mid-autumn-$($theme.version).morrowplugin"
$themePackage = Join-Path $projectRoot "build/mid-autumn-plugin/bundle/$themeName"
$installedTheme = Join-Path $releaseRoot "plugins/$themeName"
if (-not (Test-Path -LiteralPath $themePackage -PathType Leaf) -or
    -not (Test-Path -LiteralPath $installedTheme -PathType Leaf) -or
    (Get-FileHash -LiteralPath $themePackage -Algorithm SHA256).Hash -ne
      (Get-FileHash -LiteralPath $installedTheme -Algorithm SHA256).Hash) {
  throw 'Mid-Autumn theme was not built and installed from the same package'
}
Copy-Item -LiteralPath $themePackage -Destination (Join-Path $destination "plugins/$themeName")
Copy-Item -LiteralPath LICENSE,NOTICE -Destination $destination -Force
Copy-Item -LiteralPath packaging/THIRD_PARTY_NOTICES.txt -Destination $destination -Force
$sourceNotice = @"
Morrow $version - AGPL-3.0-only
Local testing preview, built from the working tree (including uncommitted changes).
Base Git commit: $sourceCommit
Corresponding source archive, supplied alongside this preview:
$([IO.Path]::GetFileName($sourceArchive))
SHA-256: $sourceHash
The snapshot was checked against the working tree before and after compilation.
This preview does not assert that a Git tag or GitHub Release exists.
Build instructions: README.md and tool/build_rust_workbench_windows.ps1
The source is available at no charge. Third-party source locations and
license notices are listed in THIRD_PARTY_NOTICES.txt and licenses/.
"@
[IO.File]::WriteAllText((Join-Path $destination 'SOURCE.txt'), $sourceNotice, [Text.UTF8Encoding]::new($false))

$licenses = Join-Path $destination licenses
New-Item -ItemType Directory -Force -Path $licenses | Out-Null
Copy-Item -Path packaging/licenses/* -Destination $licenses -Recurse -Force
$toolchain = & rustc --print sysroot
if ($LASTEXITCODE -ne 0) {throw 'Cannot locate Rust library notices'}
Copy-Item -LiteralPath (Join-Path $toolchain 'share/doc/rust/COPYRIGHT-library.html') -Destination (Join-Path $licenses 'Rust-COPYRIGHT-library.html') -Force
$metadata = & cargo metadata --offline --locked --manifest-path workbench_host/Cargo.toml --format-version 1
if ($LASTEXITCODE -ne 0) {throw 'Cannot inventory Rust libraries'}
$inventory = foreach ($package in ($metadata | ConvertFrom-Json).packages | Where-Object {$_.source}) {
  $licenseFolder = Join-Path $licenses ('cargo/'+$package.name+'-'+$package.version)
  New-Item -ItemType Directory -Force -Path $licenseFolder | Out-Null
  Get-ChildItem -LiteralPath (Split-Path $package.manifest_path) -File | Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)'} | Copy-Item -Destination $licenseFolder -Force
  "$($package.name) $($package.version): $($package.license) $($package.repository)"
}
[IO.File]::WriteAllLines((Join-Path $licenses 'cargo-dependencies.txt'),$inventory,[Text.UTF8Encoding]::new($false))
Checked python @('-X','utf8','tool/package_preview_source.py',$sourceArchive,'--check')
$files = Get-ChildItem -LiteralPath $destination -Recurse -File | Where-Object {$_.Name -ne 'SHA256SUMS.txt'}
$sums = foreach ($file in $files) {"$((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetRelativePath($destination,$file.FullName).Replace('\','/'))"}
[IO.File]::WriteAllLines((Join-Path $destination 'SHA256SUMS.txt'),$sums,[Text.UTF8Encoding]::new($false))
Compress-Archive -Path "$destination/*" -DestinationPath $archive -CompressionLevel Optimal
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
Write-Output "Built $archive"
