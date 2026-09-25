$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
  New-Item -ItemType Directory -Force build/web-identity-parity | Out-Null
  Copy-Item web/workbench/device-identity.mjs build/web-identity-parity/device-identity.mjs -Force
  Copy-Item core-web/web/identity-worker.mjs build/web-identity-parity/identity-worker.mjs -Force
  Copy-Item core-web/web/identity-index.html build/web-identity-parity/index.html -Force
  & node tool/test_core_browser.mjs --identity
  if ($LASTEXITCODE -ne 0) { throw 'Actual browser identity qualification failed' }
} finally { Pop-Location }
