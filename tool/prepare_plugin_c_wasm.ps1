# Download a fixed official sysroot into build/tools; never modifies system LLVM.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$repo=Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
 $directory=Join-Path $repo 'build/tools/wasi-34'
 New-Item -ItemType Directory -Force $directory | Out-Null
 $archive=Join-Path $directory 'wasi-sysroot-34.0.tar.gz'
 # Public official release asset; the API avoids a failing github.com download redirect.
 $url='https://api.github.com/repos/WebAssembly/wasi-sdk/releases/assets/528366552?download=1'
 $headers=@{Accept='application/octet-stream'}
 $expected='9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be'
 if(-not (Test-Path -LiteralPath $archive)) {Invoke-WebRequest $url -Headers $headers -OutFile $archive}
 elseif((Get-Item -LiteralPath $archive).Length -lt 119283116) {Invoke-WebRequest $url -Headers $headers -OutFile $archive -Resume}
 if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) {throw 'Official sysroot archive hash mismatch; extraction refused'}
 & tar -xf $archive -C $directory
 if($LASTEXITCODE -ne 0){throw 'Official sysroot extraction failed'}
 Write-Output "Verified WASI SDK 34 sysroot in $directory"
} finally {Pop-Location}
