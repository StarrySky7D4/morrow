param([string]$Python='python',[switch]$Web)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "Plugin UI verification failed: $Program"}}
Push-Location $repo
try {
 Checked $Python @('-X','utf8','tool/generate_core_client.py','--check')
 Checked cargo @('fmt','--manifest-path','core/Cargo.toml','--check')
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','ui_vectors','--','generate','build/ui-protocol')
 foreach($name in @('document.capnp','expected-event.capnp')){
   $actual=(Get-FileHash -Algorithm SHA256 (Join-Path 'build/ui-protocol' $name)).Hash
   $fixture=(Get-FileHash -Algorithm SHA256 (Join-Path 'packages/morrow_core_client/test/fixtures/ui' $name)).Hash
   if($actual -ne $fixture){throw "Stale UI fixture: $name"}
 }
 Checked flutter @('analyze','--no-pub','packages/morrow_core_client')
 Push-Location packages/morrow_core_client
 try {
  Checked dart @('test')
  Checked dart @('run','bin/ui_probe.dart',(Join-Path $repo 'build/ui-protocol'))
 }finally{Pop-Location}
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','ui_vectors','--','check','build/ui-protocol')
 if($Web){
  New-Item -ItemType Directory -Force build/ui-protocol/web | Out-Null
  Push-Location packages/morrow_core_client
  try {Checked dart @('compile','wasm','bin/ui_web_probe.dart','-o','../../build/ui-protocol/web/probe.wasm')}finally{Pop-Location}
  Copy-Item -LiteralPath packages/morrow_core_client/bin/ui_web_probe.html -Destination build/ui-protocol/web/index.html -Force
  foreach($name in @('document.capnp','expected-event.capnp')){Copy-Item -LiteralPath (Join-Path 'build/ui-protocol' $name) -Destination (Join-Path 'build/ui-protocol/web' $name) -Force}
  Checked node @('tool/test_core_browser.mjs','--ui')
  Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','ui_vectors','--','check-web','build/ui-protocol')
 }
}finally{Pop-Location}
