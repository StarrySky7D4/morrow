param([string]$Studio='C:\Program Files\Huawei\DevEco Studio')
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$env:DEVECO_SDK_HOME=Join-Path $Studio 'sdk'
$env:JAVA_HOME=Join-Path $Studio 'jbr'
$env:Path="$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $Studio 'tools/node');$env:Path"
Push-Location $root
try {
  & (Join-Path $Studio 'tools/node/node.exe') (Join-Path $Studio 'tools/hvigor/bin/hvigorw.js') assembleHap --mode module -p module=entry@default -p product=default -p buildMode=debug --no-daemon
  if($LASTEXITCODE) {throw 'HAP build failed'}
} finally {Pop-Location}
