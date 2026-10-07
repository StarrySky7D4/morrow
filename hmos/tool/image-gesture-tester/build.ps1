param([string]$Studio='C:\Program Files\Huawei\DevEco Studio')
$ErrorActionPreference='Stop'
$testerNode=Join-Path $Studio 'tools/node/node.exe'
$testerHvigor=Join-Path $Studio 'tools/hvigor/bin/hvigorw.js'
$testerProject=(& $testerNode (Join-Path $PSScriptRoot 'prepare.cjs') | ConvertFrom-Json).project
if($LASTEXITCODE) {throw 'Independent tester preparation failed'}
$env:DEVECO_SDK_HOME=Join-Path $Studio 'sdk'
$env:JAVA_HOME=Join-Path $Studio 'jbr'
$env:Path="$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $Studio 'tools/node');$env:Path"
Push-Location $testerProject
try {
  & $testerNode $testerHvigor assembleHap --mode module -p module=entry@default -p product=default -p buildMode=debug --no-daemon 2>&1 | Tee-Object -FilePath (Join-Path $testerProject 'main-build.log')
  if($LASTEXITCODE) {throw 'Independent tester main HAP build failed'}
  & $testerNode $testerHvigor assembleHap --mode module -p module=entry@ohosTest -p product=default -p buildMode=debug --no-daemon 2>&1 | Tee-Object -FilePath (Join-Path $testerProject 'test-build.log')
  if($LASTEXITCODE) {throw 'Independent tester ohosTest HAP build failed'}
  $testerArtifacts=@(Get-ChildItem -LiteralPath (Join-Path $testerProject 'entry/build') -Recurse -File -Filter '*unsigned.hap' | ForEach-Object {
    [PSCustomObject]@{Path=$_.FullName;Bytes=$_.Length;SHA256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
  })
  [PSCustomObject]@{Project=$testerProject;BundleName='dev.morrow.hmos.gesturetester';Artifacts=$testerArtifacts;Device='NOT_RUN';Installation='NOT_RUN'} | ConvertTo-Json -Depth 5
} finally {Pop-Location}
