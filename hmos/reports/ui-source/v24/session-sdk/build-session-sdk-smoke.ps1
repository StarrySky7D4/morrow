param([string]$Project, [string]$Studio = 'C:\Program Files\Huawei\DevEco Studio')
$ErrorActionPreference = 'Stop'
$sessionNode = Join-Path $Studio 'tools/node/node.exe'
$sessionHvigor = Join-Path $Studio 'tools/hvigor/bin/hvigorw.js'
if (-not $Project) {
  $sessionPrepared = & $sessionNode (Join-Path $PSScriptRoot 'prepare-session-sdk-smoke.cjs') | ConvertFrom-Json
  if ($LASTEXITCODE) { throw 'Independent session smoke preparation failed' }
  $Project = $sessionPrepared.project
}
$sessionProject = [IO.Path]::GetFullPath($Project)
$sessionWork = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../../.build/checkpoint-sdk-smoke'))
if (-not $sessionProject.StartsWith($sessionWork + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
  throw 'Compile only a fresh isolated session SDK smoke project'
}
$sessionManifest = Get-Content -LiteralPath (Join-Path $sessionProject 'source-copy-manifest.json') -Raw | ConvertFrom-Json
if ($sessionManifest.bundle_name -ne 'dev.morrow.hmos.editorbusinesssessionsdk' -or $sessionManifest.session_sdk_smoke.schema -ne 1) {
  throw 'Exact session compile-only manifest required'
}
if (Test-Path -LiteralPath (Join-Path $sessionProject 'sdk-build.log')) { throw 'Preserve prior evidence; use a new project' }
& $sessionNode (Join-Path $PSScriptRoot 'verify-session-sdk-smoke.cjs') $sessionProject |
  Set-Content -LiteralPath (Join-Path $sessionProject 'verify-before-build.json') -Encoding utf8
if ($LASTEXITCODE) { throw 'Before-build snapshot verification failed' }
$env:DEVECO_SDK_HOME = Join-Path $Studio 'sdk'
$env:JAVA_HOME = Join-Path $Studio 'jbr'
$env:Path = "$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $Studio 'tools/node');$env:Path"
Push-Location $sessionProject
try {
  & $sessionNode $sessionHvigor assembleHap --mode module -p module=entry@default -p product=default -p buildMode=debug --no-daemon 2>&1 |
    Tee-Object -FilePath (Join-Path $sessionProject 'sdk-build.log')
  $sessionExit = $LASTEXITCODE
  & $sessionNode (Join-Path $PSScriptRoot 'verify-session-sdk-smoke.cjs') $sessionProject |
    Set-Content -LiteralPath (Join-Path $sessionProject 'verify-after-build.json') -Encoding utf8
  $sessionVerifyExit = $LASTEXITCODE
  $sessionArtifacts = @()
  if (Test-Path -LiteralPath (Join-Path $sessionProject 'entry/build')) {
    $sessionArtifacts = @(Get-ChildItem -LiteralPath (Join-Path $sessionProject 'entry/build') -Recurse -File -Filter '*unsigned.hap' | ForEach-Object {
      [PSCustomObject]@{path=$_.FullName;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
    })
  }
  $sessionResult = [PSCustomObject]@{project=$sessionProject;bundle_name=$sessionManifest.bundle_name;api='26.0.0';daemon=$false;
    exit_code=$sessionExit;verification_exit_code=$sessionVerifyExit;artifacts=$sessionArtifacts;
    runtime='NOT_RUN';device='NOT_RUN';installation='NOT_RUN';qualification='Independent EditorSession SDK compile only; no Index or native runtime qualification'}
  $sessionResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $sessionProject 'sdk-build-result.json') -Encoding utf8
  $sessionResult | ConvertTo-Json -Depth 6
  if ($sessionExit -ne 0 -or $sessionVerifyExit -ne 0) { throw 'Independent SDK build or exact snapshot verification failed; preserve diagnostics' }
} finally { Pop-Location }

