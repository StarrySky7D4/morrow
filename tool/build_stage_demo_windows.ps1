param(
 [string]$Crt='C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT',
 [string]$Sysroot='build/tools/wasi-34/wasi-sysroot-34.0'
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
function Checked([string]$Program,[string[]]$Arguments){& $Program @Arguments;if($LASTEXITCODE -ne 0){throw "Stage demo build failed: $Program"}}
Push-Location $repo
try{
 $bundle=Join-Path $repo 'dist/morrow-0.1.9-test.10-demo-windows'
 $evidence=Join-Path $repo 'build/stage-demo-evidence'
 New-Item -ItemType Directory -Force $bundle,$evidence,(Join-Path $bundle 'plugins') | Out-Null
 Checked cargo @('clippy','--locked','--manifest-path','plugin_runtime/Cargo.toml','--features','packages','--example','stage_demo_host','--target-dir','build/plugin-runtime','--','-D','warnings')
 Checked cargo @('build','--locked','--release','--manifest-path','plugin_runtime/Cargo.toml','--features','packages','--example','stage_demo_host','--target-dir','build/plugin-runtime')
 Checked cargo @('build','--locked','--release','--manifest-path','sdk/examples/rust-ui/Cargo.toml','--target','wasm32-unknown-unknown','--target-dir','build/plugin-guest')
 & ./tool/build_plugin_c_wasm.ps1 -Sysroot $Sysroot
 $packages=Join-Path 'build/stage-demo-packages' ([guid]::NewGuid().ToString('N'))
 New-Item -ItemType Directory -Force $packages | Out-Null
 foreach($guest in @(@('rust','build/plugin-guest/wasm32-unknown-unknown/release/morrow_example_ui.wasm'),@('c','build/plugin-c-guest/c_ui.wasm'),@('cpp','build/plugin-c-guest/cpp_ui.wasm'))){
  $package=Join-Path $packages ($guest[0]+'.mplugin')
  Checked cargo @('run','--locked','--manifest-path','core/Cargo.toml','--target-dir','build/core-test.10','--example','plugin_package','--','pack-transform',$guest[1],$package,('org.morrow.example.'+$guest[0]+'-ui'),'0.1.9-test.10','ui.form,text.utf8,morrow.ui.document.v1,32,65536;ui.edit,morrow.ui.event.v1,morrow.ui.document.v1,65536,65536')
  Copy-Item -LiteralPath $package -Destination (Join-Path $bundle ('plugins/'+$guest[0]+'.mplugin')) -Force
 }
 Push-Location demos/plugin_stage_windows
 try{
  Checked flutter @('pub','get','--offline')
  Checked flutter @('analyze','--no-pub')
  Checked flutter @('build','windows','--release','--no-pub')
 }finally{Pop-Location}
 Copy-Item -Path demos/plugin_stage_windows/build/windows/x64/runner/Release/* -Destination $bundle -Recurse -Force
 Copy-Item -LiteralPath build/plugin-runtime/release/examples/stage_demo_host.exe -Destination $bundle -Force
 foreach($dll in @('msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll')){Copy-Item -LiteralPath (Join-Path $Crt $dll) -Destination $bundle -Force}
 $priorRoot=$env:MORROW_STAGE_DEMO_ROOT
 $env:MORROW_STAGE_DEMO_ROOT=$bundle
 Push-Location demos/plugin_stage_windows
 try{Checked flutter @('test','--no-pub','test/demo_test.dart')}finally{Pop-Location;$env:MORROW_STAGE_DEMO_ROOT=$priorRoot}
 $app=Start-Process -FilePath (Join-Path $bundle 'MorrowStageDemo.exe') -ArgumentList ('--showcase="'+$evidence+'"') -WorkingDirectory $bundle -PassThru
 if(-not $app.WaitForExit(60000)){throw "Demo is still running (PID $($app.Id)); inspect before rebuilding"}
 if($app.ExitCode -ne 0){throw 'Actual release self-check failed'}
 $check=Get-Content -LiteralPath (Join-Path $evidence 'runtime-check.txt') -Raw
 if(-not $check.StartsWith('PASS:')){throw $check}
 New-Item -ItemType Directory -Force (Join-Path $bundle '展示截图') | Out-Null
 foreach($name in @('rust-dark','c-dark','cpp-dark','light','narrow')){Copy-Item -LiteralPath (Join-Path $evidence ($name+'.png')) -Destination (Join-Path $bundle '展示截图') -Force}
 Copy-Item -LiteralPath demos/plugin_stage_windows/README.md -Destination (Join-Path $bundle '使用说明.md') -Force
 Copy-Item -LiteralPath (Join-Path $evidence 'runtime-check.txt') -Destination (Join-Path $bundle '运行验证.txt') -Force
 Copy-Item -LiteralPath LICENSE,NOTICE -Destination $bundle -Force
 Copy-Item -LiteralPath demos/plugin_stage_windows/THIRD_PARTY_NOTICES.txt -Destination $bundle -Force
 $licenses=Join-Path $bundle 'licenses'
 New-Item -ItemType Directory -Force $licenses | Out-Null
 $toolchain= & rustc --print sysroot
 if($LASTEXITCODE -ne 0){throw 'Cannot locate Rust license notices'}
 Copy-Item -LiteralPath (Join-Path $toolchain 'share/doc/rust/COPYRIGHT-library.html') -Destination (Join-Path $licenses 'Rust-COPYRIGHT-library.html') -Force
 $metadata= & cargo metadata --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --format-version 1
 if($LASTEXITCODE -ne 0){throw 'Cannot inventory Rust dependencies'}
 $inventory=foreach($package in ($metadata | ConvertFrom-Json).packages | Where-Object {$_.source}){
  $destination=Join-Path $licenses ('cargo/'+$package.name+'-'+$package.version)
  New-Item -ItemType Directory -Force $destination | Out-Null
  Get-ChildItem -LiteralPath (Split-Path $package.manifest_path) -File | Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)'} | Copy-Item -Destination $destination -Force
  "$($package.name) $($package.version): $($package.license) $($package.repository)"
 }
 [IO.File]::WriteAllLines((Join-Path $licenses 'cargo-dependencies.txt'),$inventory,[Text.UTF8Encoding]::new($false))
 $files=Get-ChildItem -LiteralPath $bundle -Recurse -File | Where-Object {$_.Name -ne 'SHA256SUMS.txt'}
 $sums=foreach($file in $files){$hash=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLower();$relative=[IO.Path]::GetRelativePath($bundle,$file.FullName).Replace('\','/');"$hash  $relative"}
 [IO.File]::WriteAllLines((Join-Path $bundle 'SHA256SUMS.txt'),$sums,[Text.UTF8Encoding]::new($false))
 $zip=$bundle+'.zip'
 Compress-Archive -Path $bundle -DestinationPath $zip -Force
 $archiveHash=Get-FileHash -LiteralPath $zip -Algorithm SHA256
 [IO.File]::WriteAllText(($zip+'.sha256'),($archiveHash.Hash.ToLower()+'  '+[IO.Path]::GetFileName($zip)+"`n"),[Text.UTF8Encoding]::new($false))
 $archiveHash
}finally{Pop-Location}
