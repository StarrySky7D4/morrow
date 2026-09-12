param([switch]$SkipVerify, [switch]$RefreshArtifact)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Program failed: $LASTEXITCODE" }
}
Checked cargo @('build','--locked','--manifest-path','plugins/workbench/Cargo.toml','--target','wasm32-unknown-unknown','--release','--target-dir','build/first-party-plugins')
Checked cargo @('build','--locked','--manifest-path','workbench_host/Cargo.toml','--release','--target-dir','build/workbench-host')
New-Item -ItemType Directory -Force -Path build/workbench-host/bundle | Out-Null
Checked build/workbench-host/release/package.exe @('build/first-party-plugins/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm','build/workbench-host/bundle/workbench.morrowplugin')
Checked python @('-X','utf8','tool/generate_workbench_client.py','--check')
if (-not $SkipVerify) {
  $env:MORROW_WORKBENCH_HOST = (Resolve-Path build/workbench-host/release/morrow-workbench-host.exe).Path
  $env:MORROW_WORKBENCH_PACKAGE = (Resolve-Path build/workbench-host/bundle/workbench.morrowplugin).Path
  Checked flutter @('test','--no-pub','test/rust_workbench_integration_test.dart')
}
Checked flutter @('build','windows','--release','--target','lib/main.dart')
$version = [regex]::Match((Get-Content pubspec.yaml -Raw),'(?m)^version: ([^+\r\n]+)').Groups[1].Value
$destination = Join-Path $projectRoot "dist/morrow-$version-rust-workbench-windows"
if ((Test-Path -LiteralPath $destination) -and -not $RefreshArtifact) { throw "Preserving existing artifact: $destination" }
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$releaseRoot = (Resolve-Path build/windows/x64/runner/Release).Path
foreach ($file in Get-ChildItem -LiteralPath $releaseRoot -Recurse -File) {
  $targetFile = Join-Path $destination ([IO.Path]::GetRelativePath($releaseRoot, $file.FullName))
  New-Item -ItemType Directory -Force -Path (Split-Path $targetFile) | Out-Null
  Copy-Item -LiteralPath $file.FullName -Destination $targetFile -Force
}
Copy-Item -LiteralPath build/workbench-host/release/morrow-workbench-host.exe -Destination $destination
New-Item -ItemType Directory -Force -Path (Join-Path $destination plugins) | Out-Null
Copy-Item -LiteralPath build/workbench-host/bundle/workbench.morrowplugin -Destination (Join-Path $destination plugins/workbench.morrowplugin)
$archive = "$destination.zip"
if ((Test-Path -LiteralPath $archive) -and -not $RefreshArtifact) {throw "Preserving existing archive: $archive"}
Copy-Item -LiteralPath LICENSE,NOTICE -Destination $destination -Force
Copy-Item -LiteralPath packaging/THIRD_PARTY_NOTICES.txt -Destination $destination -Force
$sourceNotice = @"
Morrow $version - AGPL-3.0-only
Corresponding source and build scripts:
https://github.com/StarrySky7D4/morrow/tree/v$version
Source archive:
https://github.com/StarrySky7D4/morrow/archive/refs/tags/v$version.zip
Build instructions: README.md and tool/build_rust_workbench_windows.ps1
The source is available at no charge. Third-party source locations and
license notices are listed in THIRD_PARTY_NOTICES.txt and licenses/.
"@
[IO.File]::WriteAllText((Join-Path $destination 'SOURCE.txt'), $sourceNotice, [Text.UTF8Encoding]::new($false))

$licenses = Join-Path $destination licenses
New-Item -ItemType Directory -Force -Path $licenses | Out-Null
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
$files = Get-ChildItem -LiteralPath $destination -Recurse -File | Where-Object {$_.Name -ne 'SHA256SUMS.txt'}
$sums = foreach ($file in $files) {"$((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetRelativePath($destination,$file.FullName).Replace('\','/'))"}
[IO.File]::WriteAllLines((Join-Path $destination 'SHA256SUMS.txt'),$sums,[Text.UTF8Encoding]::new($false))
Compress-Archive -Path "$destination/*" -DestinationPath $archive -CompressionLevel Optimal -Force
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
Write-Output "Built $archive"
