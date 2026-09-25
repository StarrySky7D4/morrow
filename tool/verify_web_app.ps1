param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
  if (!$SkipBuild) { & ./tool/build_web_workbench.ps1 -BaseHref /preview/ }
  foreach ($variant in @('fresh','legacy','orphan')) {
    $options = @('tool/test_core_browser.mjs','--app')
    if ($variant -ne 'fresh') { $options += "--$variant" }
    & node @options
    if ($LASTEXITCODE -ne 0) { throw "Formal Web application qualification failed: $variant" }
  }
} finally {Pop-Location}
