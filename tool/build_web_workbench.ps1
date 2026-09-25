param([string]$BaseHref = '/')
$ErrorActionPreference = 'Stop'
if ($BaseHref -notmatch '^/([A-Za-z0-9_.-]+/)*$') { throw 'BaseHref must be an absolute directory URL path ending in /' }
$repo = Split-Path $PSScriptRoot -Parent
$webOutput = Join-Path $repo 'build/web'
$previousCc = $env:CC_wasm32_unknown_unknown
$previousAr = $env:AR_wasm32_unknown_unknown
function Checked([string]$Program, [string[]]$Arguments) {
  & $Program @Arguments
  if ($LASTEXITCODE -ne 0) { throw "Web build failed: $Program" }
}
Push-Location $repo
try {
  $env:CC_wasm32_unknown_unknown = (Get-Command clang).Source
  $env:AR_wasm32_unknown_unknown = (Get-Command llvm-ar).Source
  New-Item -ItemType Directory -Force build/web-workbench-bundle | Out-Null
  Checked cargo @('build','--locked','--release','--manifest-path','plugins/workbench/Cargo.toml','--target','wasm32-unknown-unknown','--target-dir','build/first-party-plugins')
  Checked cargo @('run','--locked','--release','--manifest-path','workbench_host/Cargo.toml','--target-dir','build/workbench-host','--bin','package','--','build/first-party-plugins/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm','build/web-workbench-bundle/workbench.morrowplugin')
  Checked cargo @('build','--locked','--release','--manifest-path','core-web/Cargo.toml','--target','wasm32-unknown-unknown','--target-dir','build/web-channel-core')
  # Flutter compares output names literally during configuration cleanup. A
  # previous relative --output aliases this absolute directory and can cause
  # freshly copied assets to be deleted. Retire only that obsolete build marker.
  $marker = Join-Path $webOutput '.last_build_id'
  if (Test-Path -LiteralPath $marker) {
    $previousBuild = (Get-Content -LiteralPath $marker -Raw).Trim()
    if ($previousBuild -match '^[a-f0-9]{32}$') {
      $previousOutputs = Join-Path $repo ".dart_tool/flutter_build/$previousBuild/outputs.json"
      if (Test-Path -LiteralPath $previousOutputs) {
        $paths = Get-Content -LiteralPath $previousOutputs -Raw | ConvertFrom-Json
        if (@($paths | Where-Object { ![IO.Path]::IsPathFullyQualified($_) }).Count -gt 0) {
          Remove-Item -LiteralPath $marker
        }
      }
    }
  }
  Checked flutter @('build','web','--no-pub','--release','--no-web-resources-cdn','--base-href',$BaseHref,'--target','lib/main.dart','--output',$webOutput)
  $requiredAssets = @('index.html','main.dart.js','manifest.json','assets/FontManifest.json','assets/AssetManifest.bin')
  $languages = Get-Content packages/morrow_i18n/languages.json -Raw | ConvertFrom-Json
  foreach ($language in $languages.languages) { $requiredAssets += "assets/packages/morrow_i18n/assets/languages/$($language.code).mlang" }
  foreach ($asset in $requiredAssets) {
    if (!(Test-Path -LiteralPath (Join-Path $webOutput $asset) -PathType Leaf)) { throw "Missing Web build asset: $asset" }
  }
  New-Item -ItemType Directory -Force build/web/workbench/core | Out-Null
  Checked build/tools/wasm-bindgen/bin/wasm-bindgen.exe @('build/web-channel-core/wasm32-unknown-unknown/release/morrow_web_core.wasm','--target','web','--out-dir','build/web/workbench/core')
  Copy-Item build/web-workbench-bundle/workbench.morrowplugin build/web/workbench/workbench.morrowplugin -Force
  Copy-Item web/workbench/host-worker.mjs,web/workbench/device-identity.mjs,web/workbench/library-worker.mjs build/web/workbench/ -Force
  Write-Output 'Web application built with matching local core and bundled workbench in build/web'
} finally {
  $env:CC_wasm32_unknown_unknown = $previousCc
  $env:AR_wasm32_unknown_unknown = $previousAr
  Pop-Location
}
