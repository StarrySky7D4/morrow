param([string]$Project, [string]$Studio = 'C:\Program Files\Huawei\DevEco Studio')
$ErrorActionPreference = 'Stop'
$forkNode = Join-Path $Studio 'tools/node/node.exe'
$forkHvigor = Join-Path $Studio 'tools/hvigor/bin/hvigorw.js'
if (-not $Project) {
  $forkPrepared = & $forkNode (Join-Path $PSScriptRoot 'prepare-fork-sdk-smoke.cjs') | ConvertFrom-Json
  if ($LASTEXITCODE) { throw 'Fork SDK smoke preparation failed' }
  $Project = $forkPrepared.project
}
$forkProject = [IO.Path]::GetFullPath($Project)
$forkWork = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../../.build/checkpoint-sdk-smoke'))
if (-not $forkProject.StartsWith($forkWork + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
  throw 'Compile only a fresh isolated fork SDK smoke project'
}
$forkManifestPath = Join-Path $forkProject 'source-copy-manifest.json'
$forkManifest = Get-Content -LiteralPath $forkManifestPath -Raw | ConvertFrom-Json
if ($forkManifest.bundle_name -ne 'dev.morrow.hmos.draftforksdk' -or $forkManifest.fork_sdk_smoke.schema -ne 1) {
  throw 'Explicit fork harness manifest required'
}
if (Test-Path -LiteralPath (Join-Path $forkProject 'sdk-build.log')) { throw 'Preserve earlier build evidence; prepare a new project' }
& $forkNode (Join-Path $PSScriptRoot 'verify-fork-sdk-smoke.cjs') $forkProject | Set-Content -LiteralPath (Join-Path $forkProject 'verify-before-build.json') -Encoding utf8
if ($LASTEXITCODE) { throw 'Fresh copy verification failed' }
$env:DEVECO_SDK_HOME = Join-Path $Studio 'sdk'
$env:JAVA_HOME = Join-Path $Studio 'jbr'
$env:Path = "$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $Studio 'tools/node');$env:Path"
Push-Location $forkProject
try {
  & $forkNode $forkHvigor assembleHap --mode module -p module=entry@default -p product=default -p buildMode=debug --no-daemon 2>&1 |
    Tee-Object -FilePath (Join-Path $forkProject 'sdk-build.log')
  $forkExit = $LASTEXITCODE
  $forkArtifacts = @()
  if (Test-Path -LiteralPath (Join-Path $forkProject 'entry/build')) {
    $forkArtifacts = @(Get-ChildItem -LiteralPath (Join-Path $forkProject 'entry/build') -Recurse -File -Filter '*unsigned.hap' | ForEach-Object {
      [PSCustomObject]@{path=$_.FullName;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
    })
  }
  $forkResult = [PSCustomObject]@{project=$forkProject;bundle_name=$forkManifest.bundle_name;api='26.0.0';daemon=$false;
    exit_code=$forkExit;artifacts=$forkArtifacts;runtime='NOT_RUN';device='NOT_RUN';installation='NOT_RUN';
    qualification='Independent fork SDK compile only; copied CPP is not native fork identity or runtime proof'}
  $forkResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $forkProject 'sdk-build-result.json') -Encoding utf8
  $forkResult | ConvertTo-Json -Depth 6
  if ($forkExit -ne 0) { throw 'Independent fork SDK compile failed; preserve diagnostics' }
} finally { Pop-Location }
