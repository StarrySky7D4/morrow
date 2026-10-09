param([Parameter(Mandatory=$true)][string]$Label)
$ErrorActionPreference = 'Stop'
if ($Label -notmatch '^dev33-preferences-tasks-[a-z0-9-]+$') { throw 'Invalid immutable build label' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$taskRepo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$taskHmos = Join-Path $taskRepo 'hmos'
$taskProofPath = Join-Path $PSScriptRoot ($Label + '-package-check.json')
if (Test-Path -LiteralPath $taskProofPath) { throw 'Preserve earlier package proof' }
$taskArtifact = Get-Content -LiteralPath (Join-Path $PSScriptRoot ($Label + '-artifact.json')) -Raw | ConvertFrom-Json
$taskProject = Join-Path $taskHmos ('.build/device-candidates/' + $Label)
$taskHap = Join-Path $taskHmos $taskArtifact.path
function Hash-TaskBytes([byte[]]$TaskBytes) {
  $taskHasher = [Security.Cryptography.SHA256]::Create()
  try { return [Convert]::ToHexString($taskHasher.ComputeHash($TaskBytes)) } finally { $taskHasher.Dispose() }
}
function Task-FileRecord([string]$TaskRelative) {
  $taskBytes = [IO.File]::ReadAllBytes((Join-Path $taskProject $TaskRelative))
  return [ordered]@{ path=$TaskRelative; bytes=$taskBytes.Length; sha256=(Hash-TaskBytes $taskBytes) }
}
$taskHapBytes = [IO.File]::ReadAllBytes($taskHap)
if ($taskHapBytes.Length -ne $taskArtifact.bytes -or (Hash-TaskBytes $taskHapBytes) -cne $taskArtifact.sha256) { throw 'Artifact drift' }
$taskZip = [IO.Compression.ZipFile]::OpenRead($taskHap)
function Task-ZipBytes([string]$TaskName) {
  $taskEntry = $taskZip.GetEntry($TaskName)
  if ($null -eq $taskEntry) { throw ('Missing package entry ' + $TaskName) }
  $taskStream = $taskEntry.Open()
  $taskMem = [IO.MemoryStream]::new()
  try { $taskStream.CopyTo($taskMem); return ,$taskMem.ToArray() }
  finally { $taskMem.Dispose(); $taskStream.Dispose() }
}
try {
  $taskApp = ([Text.Encoding]::UTF8.GetString((Task-ZipBytes 'module.json')) | ConvertFrom-Json).app
  if ($taskApp.versionCode -ne 1000023 -or $taskApp.versionName -cne '0.1.0-hmos-dev.23' -or $taskApp.bundleName -cne 'dev.morrow.hmos') { throw 'Unexpected packaged identity' }
  $taskAbc = Task-ZipBytes 'ets/modules.abc'
  $taskAbcText = [Text.Encoding]::UTF8.GetString($taskAbc)
  $taskEmitBase = 'entry/build/default/cache/default/default@CompileArkTS/esmodule/debug/'
  $taskFilesInfo = [IO.File]::ReadAllBytes((Join-Path $taskProject ($taskEmitBase + 'filesInfo.txt')))
  $taskFilesInfoText = [Text.Encoding]::UTF8.GetString($taskFilesInfo)
  $taskNames = @('model/MusicFiles','model/MusicLibrary','model/MusicPlayback','model/MusicUi','model/MusicWorkbench',
    'pages/PlatformMusicPlayer','pages/MusicPanel','pages/MusicFooter','pages/RecessedGlassRelief','model/AppearancePreferences','pages/Index')
  $taskModules = @()
  foreach ($taskName in $taskNames) {
    $taskRecord = 'entry|entry|1.0.0|src/main/ets/' + $taskName + '.ts'
    $taskListed = $taskFilesInfoText.Contains('&entry/src/main/ets/' + $taskName + '&')
    $taskPackaged = $taskAbcText.Contains($taskRecord)
    if (-not $taskListed -or -not $taskPackaged) { throw ('Product graph missing ' + $taskName) }
    $taskOutputs = @()
    foreach ($taskExtension in @('.ts','.protoBin')) {
      $taskOutput = Task-FileRecord ($taskEmitBase + 'entry/src/main/ets/' + $taskName + $taskExtension)
      if ($taskOutput.bytes -le 0) { throw ('Empty emit ' + $taskName) }
      $taskOutputs += $taskOutput
    }
    $taskModules += [ordered]@{ module=$taskName; listed=$taskListed; emitted=$true; packaged=$taskPackaged; record=$taskRecord; outputs=$taskOutputs }
  }
  $taskEntries = @()
  foreach ($taskAbi in @('arm64-v8a','x86_64')) {
    foreach ($taskLib in @('libmorrow.so','libc++_shared.so')) {
      $taskName = 'libs/' + $taskAbi + '/' + $taskLib
      $taskLibBytes = Task-ZipBytes $taskName
      $taskSource = Task-FileRecord ('entry/build/default/intermediates/stripped_native_libs/default/' + $taskAbi + '/' + $taskLib)
      if ($taskLibBytes.Length -ne $taskSource.bytes -or (Hash-TaskBytes $taskLibBytes) -cne $taskSource.sha256) { throw ('Packaged native drift ' + $taskName) }
      $taskEntries += [ordered]@{ path=$taskName; bytes=$taskLibBytes.Length; sha256=(Hash-TaskBytes $taskLibBytes); source=$taskSource.path; status='PASS' }
    }
  }
  $taskProof = [ordered]@{ checkedUtc=[DateTime]::UtcNow.ToString('o'); status='PASS'; artifact=$Label;
    artifactBytes=$taskHapBytes.Length; artifactSha256=(Hash-TaskBytes $taskHapBytes); packagedApp=$taskApp;
    entries=$taskEntries; modules=$taskModules; filesInfoSha256=(Hash-TaskBytes $taskFilesInfo);
    packagedAbc=[ordered]@{ path='ets/modules.abc'; bytes=$taskAbc.Length; sha256=(Hash-TaskBytes $taskAbc) };
    device='NOT_RUN'; scope='Actual complete product graph: eight music modules, shared relief, AppearancePreferences and Index; exact package identity and four compiler-stripped native libraries. No device persistence, Task rendering, playback or full parity qualification.' }
  [IO.File]::WriteAllText($taskProofPath, ($taskProof | ConvertTo-Json -Depth 12) + "`n")
  [ordered]@{status='PASS'; modules=$taskModules.Count; native=$taskEntries.Count; artifact=$taskProof.artifactSha256} | ConvertTo-Json
} finally { $taskZip.Dispose() }
