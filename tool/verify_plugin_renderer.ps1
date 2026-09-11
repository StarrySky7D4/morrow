param([switch]$Web)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "Plugin renderer verification failed: $Program"}}
Push-Location $repo
try {
 Checked cargo @('fmt','--manifest-path','core/Cargo.toml','--check')
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','ui_vectors','--','generate','build/ui-protocol')
 $source=(Get-FileHash -Algorithm SHA256 build/ui-protocol/document.capnp).Hash
 $fixture=(Get-FileHash -Algorithm SHA256 packages/morrow_plugin_ui/test/fixtures/form.capnp).Hash
 if($source -ne $fixture){throw 'Renderer Rust fixture is stale'}
 Push-Location packages/morrow_core_client
 try {Checked dart @('run','bin/renderer_fixture.dart',(Join-Path $repo 'build/ui-protocol/document.capnp'),(Join-Path $repo 'packages/morrow_plugin_ui/example/lib/form_fixture.dart'),'--check')}finally{Pop-Location}
 Push-Location packages/morrow_plugin_ui
 try {
  Checked flutter @('analyze','--no-pub')
  Checked flutter @('test','--no-pub','test/plugin_form_test.dart')
 }finally{Pop-Location}
 Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','ui_vectors','--','check-widget','build/ui-protocol')
 if($Web){
  Push-Location packages/morrow_plugin_ui/example
  try {Checked flutter @('analyze','--no-pub');Checked flutter @('build','web','--no-pub','--release','--no-web-resources-cdn','--base-href','/preview/','--output',(Join-Path $repo 'build/ui-renderer/web'))}finally{Pop-Location}
  Checked node @('tool/test_core_browser.mjs','--renderer')
 }
}finally{Pop-Location}
