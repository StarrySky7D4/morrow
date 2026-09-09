[CmdletBinding(SupportsShouldProcess)]
param()
$ErrorActionPreference = 'Stop'

# Run after closing Codex/editor sessions and terminals that hold this project.
# Preserve path-dependent build caches after the move; never reset or clean Git.
$projectSource = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$projectParent = Split-Path $projectSource -Parent
$projectDestination = [IO.Path]::GetFullPath((Join-Path $projectParent 'morrow'))
if ($projectSource -eq $projectDestination) {
    Write-Output "Already named morrow: $projectDestination"
    return
}
if ((Split-Path $projectSource -Leaf) -notin @('daemon', 'Morron')) {
    throw 'Expected the daemon or Morron project directory. No files were moved.'
}
if ((Get-Item -LiteralPath $projectSource).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'Refusing to move a linked project directory. No files were moved.'
}
if ((Split-Path $projectDestination -Parent) -ne $projectParent) {
    throw 'Destination must stay in the same parent directory.'
}
if (Test-Path -LiteralPath $projectDestination) {
    throw "Destination already exists; refusing to merge directories: $projectDestination"
}
$projectManifest = Join-Path $projectSource 'pubspec.yaml'
if (!(Test-Path -LiteralPath $projectManifest) -or
    (Get-Content -LiteralPath $projectManifest -Raw) -notmatch '(?m)^name:\s*morrow_studio\s*$') {
    throw 'This is not the renamed Morrow project. No files were moved.'
}

if (!$PSCmdlet.ShouldProcess($projectSource, "Rename project directory to $projectDestination")) {
    return
}
Set-Location -LiteralPath $projectParent
try {
    Move-Item -LiteralPath $projectSource -Destination $projectDestination
} catch {
    throw "Directory rename failed. Close applications and Codex sessions using the project, then retry. $($_.Exception.Message)"
}
Write-Output "Renamed project: $projectDestination"
$cacheBackup = Join-Path $projectDestination ('build/rename-backup/directory-' + [guid]::NewGuid().ToString('N'))
foreach ($relativeCache in @('build/windows', 'windows/flutter/ephemeral', '.dart_tool/flutter_build')) {
    $cacheSource = [IO.Path]::GetFullPath((Join-Path $projectDestination $relativeCache))
    $cacheTarget = [IO.Path]::GetFullPath((Join-Path $cacheBackup (Split-Path $cacheSource -Leaf)))
    if (!$cacheSource.StartsWith($projectDestination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        !$cacheTarget.StartsWith($projectDestination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Cache paths must remain inside the renamed project.'
    }
    if (Test-Path -LiteralPath $cacheSource) {
        New-Item -ItemType Directory -Path $cacheBackup -Force | Out-Null
        Move-Item -LiteralPath $cacheSource -Destination $cacheTarget
        Write-Output "Preserved build cache: $cacheTarget"
    }
}
# These dependency archives contain no project paths. CMake verifies their hashes.
foreach ($archiveName in @('ANGLE.7z', 'mpv-dev-x86_64-20230924-git-652a1dd.7z')) {
    $archiveSource = Join-Path $cacheBackup "windows/x64/$archiveName"
    if (Test-Path -LiteralPath $archiveSource) {
        $archiveTarget = Join-Path $projectDestination 'build/windows/x64'
        New-Item -ItemType Directory -Path $archiveTarget -Force | Out-Null
        Copy-Item -LiteralPath $archiveSource -Destination $archiveTarget
    }
}
Write-Output 'Open this directory in Codex, run flutter pub get, then rebuild Windows/Web. Previous outputs remain in build/rename-backup.'
