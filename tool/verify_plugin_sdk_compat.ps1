param([string]$Python='python', [string]$TargetDir='build/plugin-runtime')
$ErrorActionPreference='Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'This complete frozen SDK qualification currently requires Windows; integrity-only checks remain portable.' }
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){ & $Program @Arguments; if($LASTEXITCODE -ne 0){throw "Frozen SDK compatibility failed: $Program"} }
Push-Location $repo
try {
 # Never compile the guest, run a capture tool, or rewrite the original packages here.
 Checked $Python @('tool/verify_plugin_sdk_baseline.py')
 Checked $Python @('-m','unittest','discover','-s','tool/tests','-p','test_sdk_*.py')
 Checked cargo @('test','--locked','--manifest-path','plugin_runtime/Cargo.toml','--target-dir',$TargetDir,'--release','--features','packages','--test','sdk_frozen_compat','--test','sdk_frozen_dependency','--','--nocapture')
 Checked $Python @('tool/verify_plugin_sdk_baseline.py')
} finally {Pop-Location}
