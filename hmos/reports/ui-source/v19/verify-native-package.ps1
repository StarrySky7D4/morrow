param([string]$RepositoryRoot = (Get-Location).Path)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$taskRoot=(Resolve-Path -LiteralPath $RepositoryRoot).Path
$taskHmos=Join-Path $taskRoot 'hmos'
$taskArtifact=Join-Path $taskHmos '.build/artifacts/dev19/entry-default-unsigned.hap'
$taskPrior=Join-Path $taskHmos '.build/artifacts/dev18/entry-default-unsigned.hap'
if((Get-FileHash -LiteralPath $taskPrior -Algorithm SHA256).Hash -ne '910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2'){throw 'Preserved dev18 identity mismatch'}
$taskZip=[IO.Compression.ZipFile]::OpenRead($taskArtifact)
$taskPriorZip=[IO.Compression.ZipFile]::OpenRead($taskPrior)
$taskRecords=@()
function Measure-NativeEntry($Archive,[string]$EntryName) {
  $taskEntry=$Archive.GetEntry($EntryName)
  if(-not $taskEntry){throw "Missing native entry $EntryName"}
  $taskStream=$taskEntry.Open()
  $taskSha=[Security.Cryptography.SHA256]::Create()
  try { $taskDigest=[Convert]::ToHexString($taskSha.ComputeHash($taskStream)) }
  finally { $taskStream.Dispose(); $taskSha.Dispose() }
  return @{bytes=$taskEntry.Length;sha256=$taskDigest}
}
try {
  foreach($taskAbi in @('arm64-v8a','x86_64')) {
    foreach($taskLibrary in @('libmorrow.so','libc++_shared.so')) {
      $taskEntryName="libs/$taskAbi/$taskLibrary"
      $taskCurrent=Measure-NativeEntry $taskZip $taskEntryName
      $taskOld=Measure-NativeEntry $taskPriorZip $taskEntryName
      $taskEqual=$taskCurrent.sha256 -eq $taskOld.sha256 -and $taskCurrent.bytes -eq $taskOld.bytes
      if($taskLibrary -eq 'libmorrow.so') {
        $taskLink=Join-Path $taskHmos "entry/build/default/intermediates/stripped_native_libs/default/$taskAbi/libmorrow.so"
        if((Get-FileHash -LiteralPath $taskLink -Algorithm SHA256).Hash -ne $taskCurrent.sha256 -or (Get-Item -LiteralPath $taskLink).Length -ne $taskCurrent.bytes){throw "Stripped packaging shared-library mismatch $taskAbi"}
        if($taskEqual){throw "Native library unexpectedly unchanged from dev18 $taskAbi"}
      } elseif(-not $taskEqual){throw "SDK libc++ library changed unexpectedly $taskAbi"}
      $taskRecords+=@{path=$taskEntryName;bytes=$taskCurrent.bytes;sha256=$taskCurrent.sha256;prior_bytes=$taskOld.bytes;prior_sha256=$taskOld.sha256;byte_equal=$taskEqual}
    }
  }
} finally { $taskZip.Dispose(); $taskPriorZip.Dispose() }
$taskResult=@{result='PASS';captured_utc=[DateTime]::UtcNow.ToString('o');version='0.1.0-hmos-dev.19';artifact='hmos/.build/artifacts/dev19/entry-default-unsigned.hap';bytes=(Get-Item -LiteralPath $taskArtifact).Length;sha256=(Get-FileHash -LiteralPath $taskArtifact -Algorithm SHA256).Hash;prior_artifact='hmos/.build/artifacts/dev18/entry-default-unsigned.hap';native_entries=$taskRecords;scope='All four native ZIP entries fully decompressed and SHA/length checked; both product libraries match SDK stripped packaging files and differ from dev18; libc++ entries unchanged. No installation or device claim.'}
$taskResult | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskHmos 'reports/ui-source/v19/native-package-comparison.json') -Encoding utf8NoBOM
$taskResult | Select-Object result,version,bytes,sha256 | ConvertTo-Json -Compress
