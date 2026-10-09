param([Parameter(Mandatory=$true)][ValidatePattern('^dev32-glass-alignment-[a-z0-9-]+$')][string]$ProjectLabel)
$ErrorActionPreference = 'Stop'
$hmosRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$projectRoot = Join-Path $hmosRoot ".build/device-candidates/$ProjectLabel"
$archivePath = Join-Path $hmosRoot ".build/artifacts/$ProjectLabel/entry-default-unsigned.hap"
$proofPath = Join-Path $PSScriptRoot "$ProjectLabel-package-check.json"
if (Test-Path -LiteralPath $proofPath) { throw 'Do not overwrite package evidence' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
try {
  $entries = @()
  foreach ($abi in @('arm64-v8a', 'x86_64')) {
    foreach ($library in @('libmorrow.so', 'libc++_shared.so')) {
      $entryPath = "libs/$abi/$library"; $entry = $zip.GetEntry($entryPath)
      if (-not $entry) { throw "Missing native library $entryPath" }
      $stream = $entry.Open(); $hasher = [Security.Cryptography.SHA256]::Create()
      try { $digest = [Convert]::ToHexString($hasher.ComputeHash($stream)) }
      finally { $stream.Dispose(); $hasher.Dispose() }
      $source = "entry/build/default/intermediates/stripped_native_libs/default/$abi/$library"
      $sourceFile = Get-Item -LiteralPath (Join-Path $projectRoot $source)
      if ($digest -ne (Get-FileHash -LiteralPath $sourceFile.FullName).Hash -or $entry.Length -ne $sourceFile.Length) { throw "Native package mismatch $entryPath" }
      $entries += [ordered]@{path=$entryPath;bytes=$entry.Length;sha256=$digest;source=$source;status='PASS'}
    }
  }
  $graph = 'entry/build/default/cache/default/default@CompileArkTS/esmodule/debug'
  $filesInfo = Join-Path $projectRoot "$graph/filesInfo.txt"
  $filesText = Get-Content -LiteralPath $filesInfo -Raw
  $modules = @()
  foreach ($module in @('model/MusicFiles','model/MusicLibrary','model/MusicPlayback','model/MusicUi','model/MusicWorkbench','pages/PlatformMusicPlayer','pages/MusicPanel','pages/MusicFooter','pages/RecessedGlassRelief')) {
    $fileName = $module.Split('/')[-1]
    if (-not $filesText.Contains("/$module.ts;")) { throw "Product graph does not reference $module" }
    $outputs = @()
    foreach ($suffix in @('ts', 'protoBin')) {
      $output = "$graph/entry/src/main/ets/$module.$suffix"
      $file = Get-Item -LiteralPath (Join-Path $projectRoot $output)
      if ($file.Length -le 0) { throw "Missing emitted $module.$suffix" }
      $outputs += [ordered]@{path=$output;bytes=$file.Length;sha256=(Get-FileHash -LiteralPath $file.FullName).Hash}
    }
    $modules += [ordered]@{module=$module;listed=$true;emitted=$true;outputs=$outputs}
  }
  $proof = [ordered]@{checkedUtc=[DateTime]::UtcNow.ToString('o');status='PASS';artifact=$ProjectLabel;entries=$entries;modules=$modules;
    filesInfoSha256=(Get-FileHash -LiteralPath $filesInfo).Hash;device='NOT_RUN';scope='Actual product entry graph emits all eight music modules; package native libraries match compiler stripped outputs. No device rendering or audible playback claim.'}
  $proof | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $proofPath -Encoding utf8NoBOM
  $proof | ConvertTo-Json -Depth 12
} finally { $zip.Dispose() }
