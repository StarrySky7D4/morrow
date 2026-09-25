$ErrorActionPreference = 'Stop'
if (!$env:GITHUB_ACTIONS) { throw 'This installer is for an isolated GitHub Actions runner.' }
$toolsRoot = Join-Path $env:RUNNER_TEMP 'morrow-web-tools'
New-Item -ItemType Directory -Force $toolsRoot | Out-Null
Invoke-WebRequest 'https://capnproto.org/capnproto-c++-win32-1.4.0.zip' -OutFile "$toolsRoot/capnp.zip"
Expand-Archive "$toolsRoot/capnp.zip" "$toolsRoot/capnp"
$capnp = Get-ChildItem "$toolsRoot/capnp" -Recurse -Filter capnp.exe | Select-Object -First 1
if (!$capnp) { throw 'Missing Capn Proto compiler' }
Invoke-WebRequest 'https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.7/clang+llvm-22.1.7-x86_64-pc-windows-msvc.tar.xz' -OutFile "$toolsRoot/llvm.tar.xz"
if ((Get-FileHash "$toolsRoot/llvm.tar.xz" -Algorithm SHA256).Hash.ToLowerInvariant() -ne '3b568b5be1443d1a04c63261fa3a7aed16e126a8ed2196a1032aa8ed602144bd') { throw 'LLVM archive hash mismatch' }
New-Item -ItemType Directory "$toolsRoot/llvm" | Out-Null
tar -xf "$toolsRoot/llvm.tar.xz" -C "$toolsRoot/llvm" --strip-components 1
if ($LASTEXITCODE -ne 0) { throw 'LLVM extraction failed' }
$capnp.DirectoryName | Out-File $env:GITHUB_PATH -Append -Encoding utf8
"$toolsRoot/llvm/bin" | Out-File $env:GITHUB_PATH -Append -Encoding utf8
rustup toolchain install 1.96.0 --profile minimal --target wasm32-unknown-unknown
if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain installation failed' }
rustup override set 1.96.0
if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain selection failed' }
Invoke-WebRequest 'https://storage.googleapis.com/chrome-for-testing-public/154.0.8037.57/win64/chrome-win64.zip' -OutFile "$toolsRoot/chrome.zip"
Expand-Archive "$toolsRoot/chrome.zip" "$toolsRoot/chrome"
"CHROME_BIN=$toolsRoot/chrome/chrome-win64/chrome.exe" | Out-File $env:GITHUB_ENV -Append -Encoding utf8
