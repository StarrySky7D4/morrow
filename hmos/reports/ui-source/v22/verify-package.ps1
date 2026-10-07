$ErrorActionPreference = 'Stop'
$hmosRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archivePath = Join-Path $hmosRoot '.build/artifacts/dev22-business-checkpoint/entry-default-unsigned.hap'
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
      $sourceFile = Get-Item -LiteralPath (Join-Path $hmosRoot $source)
      $sourceHash = (Get-FileHash -LiteralPath $sourceFile.FullName -Algorithm SHA256).Hash
      if ($digest -ne $sourceHash -or $entry.Length -ne $sourceFile.Length) { throw "Native package mismatch $entryPath" }
      $entries += [ordered]@{path=$entryPath;bytes=$entry.Length;sha256=$digest;source=$source;status='PASS'}
    }
  }
  $proof = [ordered]@{checkedUtc=[DateTime]::UtcNow.ToString('o');status='PASS';artifact='dev22-business-checkpoint';entries=$entries;device='NOT_RUN'}
  $proof | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'native-package-check.json') -Encoding utf8NoBOM
  $proof | ConvertTo-Json -Depth 8
} finally { $zip.Dispose() }
