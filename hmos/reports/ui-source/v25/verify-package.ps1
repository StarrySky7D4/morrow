$ErrorActionPreference = 'Stop'
$hmosRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$projectRoot = Join-Path $hmosRoot '.build/device-candidates/dev25-business-handoff-checkpoint-retry2'
$archivePath = Join-Path $hmosRoot '.build/artifacts/dev25-business-handoff-checkpoint/entry-default-unsigned.hap'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
try {
  $entries = @()
  foreach ($abi in @('arm64-v8a', 'x86_64')) {
    foreach ($library in @('libmorrow.so', 'libc++_shared.so')) {
      $entryPath = "libs/$abi/$library"
      $entry = $zip.GetEntry($entryPath)
      if (-not $entry) { throw "Missing native library $entryPath" }
      $stream = $entry.Open()
      $hasher = [Security.Cryptography.SHA256]::Create()
      try { $digest = [Convert]::ToHexString($hasher.ComputeHash($stream)) }
      finally { $stream.Dispose(); $hasher.Dispose() }
      $source = "entry/build/default/intermediates/stripped_native_libs/default/$abi/$library"
      $sourceFile = Get-Item -LiteralPath (Join-Path $projectRoot $source)
      if ($digest -ne (Get-FileHash -LiteralPath $sourceFile.FullName).Hash -or $entry.Length -ne $sourceFile.Length) { throw "Native package mismatch $entryPath" }
      $entries += [ordered]@{path=$entryPath;bytes=$entry.Length;sha256=$digest;source=$source;status='PASS'}
    }
  }
  $proof = [ordered]@{checkedUtc=[DateTime]::UtcNow.ToString('o');status='PASS';artifact='dev25-business-handoff-checkpoint';entries=$entries;device='NOT_RUN';nativeBuild='fresh v25 dual ABI native release archives'}
  $proof | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'native-package-check.json') -Encoding utf8NoBOM
  $proof | ConvertTo-Json -Depth 8
} finally { $zip.Dispose() }
