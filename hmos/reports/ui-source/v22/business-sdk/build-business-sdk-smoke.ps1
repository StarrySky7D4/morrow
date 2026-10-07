param([string]$Project, [string]$Studio = 'C:\Program Files\Huawei\DevEco Studio')
$ErrorActionPreference = 'Stop'
$businessNode = Join-Path $Studio 'tools/node/node.exe'
$businessHvigor = Join-Path $Studio 'tools/hvigor/bin/hvigorw.js'
if (-not $Project) {
  $businessPrepared = & $businessNode (Join-Path $PSScriptRoot 'prepare-business-sdk-smoke.cjs') | ConvertFrom-Json
  if ($LASTEXITCODE) { throw 'Independent business smoke preparation failed' }
  $Project = $businessPrepared.project
}
$businessProject = [IO.Path]::GetFullPath($Project)
$businessWork = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../../.build/checkpoint-sdk-smoke'))
if (-not $businessProject.StartsWith($businessWork + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
  throw 'Compile only a fresh isolated business SDK smoke project'
}
$businessManifest = Get-Content -LiteralPath (Join-Path $businessProject 'source-copy-manifest.json') -Raw | ConvertFrom-Json
if ($businessManifest.bundle_name -ne 'dev.morrow.hmos.editorbusinesssdk' -or $businessManifest.business_sdk_smoke.schema -ne 1) {
  throw 'Exact business compile-only manifest required'
}
if (Test-Path -LiteralPath (Join-Path $businessProject 'sdk-build.log')) { throw 'Preserve prior evidence; use a new project' }
& $businessNode (Join-Path $PSScriptRoot 'verify-business-sdk-smoke.cjs') $businessProject |
  Set-Content -LiteralPath (Join-Path $businessProject 'verify-before-build.json') -Encoding utf8
if ($LASTEXITCODE) { throw 'Before-build snapshot verification failed' }
$env:DEVECO_SDK_HOME = Join-Path $Studio 'sdk'
$env:JAVA_HOME = Join-Path $Studio 'jbr'
$env:Path = "$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $Studio 'tools/node');$env:Path"
Push-Location $businessProject
try {
  & $businessNode $businessHvigor assembleHap --mode module -p module=entry@default -p product=default -p buildMode=debug --no-daemon 2>&1 |
    Tee-Object -FilePath (Join-Path $businessProject 'sdk-build.log')
  $businessExit = $LASTEXITCODE
  & $businessNode (Join-Path $PSScriptRoot 'verify-business-sdk-smoke.cjs') $businessProject |
    Set-Content -LiteralPath (Join-Path $businessProject 'verify-after-build.json') -Encoding utf8
  $businessVerifyExit = $LASTEXITCODE
  $businessArtifacts = @()
  if (Test-Path -LiteralPath (Join-Path $businessProject 'entry/build')) {
    $businessArtifacts = @(Get-ChildItem -LiteralPath (Join-Path $businessProject 'entry/build') -Recurse -File -Filter '*unsigned.hap' | ForEach-Object {
      [PSCustomObject]@{path=$_.FullName;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
    })
  }
  $businessResult = [PSCustomObject]@{project=$businessProject;bundle_name=$businessManifest.bundle_name;api='26.0.0';daemon=$false;
    exit_code=$businessExit;verification_exit_code=$businessVerifyExit;artifacts=$businessArtifacts;
    runtime='NOT_RUN';device='NOT_RUN';installation='NOT_RUN';qualification='Independent EditorBusiness SDK compile only; no Index or native runtime qualification'}
  $businessResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $businessProject 'sdk-build-result.json') -Encoding utf8
  $businessResult | ConvertTo-Json -Depth 6
  if ($businessExit -ne 0 -or $businessVerifyExit -ne 0) { throw 'Independent SDK build or exact snapshot verification failed; preserve diagnostics' }
} finally { Pop-Location }
