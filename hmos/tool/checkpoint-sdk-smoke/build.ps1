param([string]$Project, [string]$Studio = 'C:\Program Files\Huawei\DevEco Studio')
$ErrorActionPreference = 'Stop'
$smokeNode = Join-Path $Studio 'tools/node/node.exe'
$smokeHvigor = Join-Path $Studio 'tools/hvigor/bin/hvigorw.js'
if (-not $Project) {
  $smokePrepared = & $smokeNode (Join-Path $PSScriptRoot 'prepare.cjs') | ConvertFrom-Json
  if ($LASTEXITCODE) { throw 'Checkpoint SDK smoke preparation failed' }
  $Project = $smokePrepared.project
}
$smokeProject = [IO.Path]::GetFullPath($Project)
$smokeWork = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../.build/checkpoint-sdk-smoke'))
if (-not $smokeProject.StartsWith($smokeWork + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
  throw 'Compile only a prepared isolated checkpoint-sdk-smoke project'
}
if (-not (Test-Path -LiteralPath (Join-Path $smokeProject 'source-copy-manifest.json'))) { throw 'Prepared copy manifest is required' }
if (Test-Path -LiteralPath (Join-Path $smokeProject 'sdk-build.log')) { throw 'Earlier build evidence is preserved; prepare a new project' }
$env:DEVECO_SDK_HOME = Join-Path $Studio 'sdk'
$env:JAVA_HOME = Join-Path $Studio 'jbr'
$env:Path = "$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $Studio 'tools/node');$env:Path"
Push-Location $smokeProject
try {
  & $smokeNode $smokeHvigor assembleHap --mode module -p module=entry@default -p product=default -p buildMode=debug --no-daemon 2>&1 |
    Tee-Object -FilePath (Join-Path $smokeProject 'sdk-build.log')
  $smokeExit = $LASTEXITCODE
  $smokeArtifacts = @()
  if (Test-Path -LiteralPath (Join-Path $smokeProject 'entry/build')) {
    $smokeArtifacts = @(Get-ChildItem -LiteralPath (Join-Path $smokeProject 'entry/build') -Recurse -File -Filter '*unsigned.hap' | ForEach-Object {
      [PSCustomObject]@{path=$_.FullName;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
    })
  }
  $smokeResult = [PSCustomObject]@{project=$smokeProject;bundle_name='dev.morrow.hmos.checkpointsdk';api='26.0.0';daemon=$false;
    exit_code=$smokeExit;artifacts=$smokeArtifacts;device='NOT_RUN';installation='NOT_RUN';qualification='Independent SDK compile only'}
  $smokeResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $smokeProject 'sdk-build-result.json') -Encoding utf8
  $smokeResult | ConvertTo-Json -Depth 6
  if ($smokeExit -ne 0) { throw 'Independent SDK compile failed; preserve diagnostics and do not treat it as product build proof' }
} finally { Pop-Location }
