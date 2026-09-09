param(
  [string]$CrtDirectory = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Redist\MSVC\14.44.35112\x64\Microsoft.VC143.CRT'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$version = [regex]::Match((Get-Content -Raw "$repo/pubspec.yaml"), '(?m)^version:\s*([0-9.]+)').Groups[1].Value
$build = Join-Path $repo 'build/windows/x64/runner/Release'
$output = Join-Path $repo "dist/daemon-$version-windows-x64.zip"
if (Test-Path $output) { throw "Already exists: $output" }
foreach ($file in @("$build/daemon_studio.exe", "$build/libmpv-2.dll", "$build/data/app.so", "$CrtDirectory/vcruntime140.dll")) {
  if (!(Test-Path -LiteralPath $file)) { throw "Missing dependency: $file" }
}
$stage = Join-Path $repo ('build/share-' + [guid]::NewGuid().ToString('N') + '/daemon')
New-Item -ItemType Directory -Force $stage, "$repo/dist" | Out-Null
Copy-Item "$build/*" $stage -Recurse
Copy-Item "$CrtDirectory/*.dll" $stage
Copy-Item "$repo/packaging/USER_GUIDE.txt" "$stage/使用说明.txt"
Copy-Item "$repo/packaging/THIRD_PARTY_NOTICES.txt" $stage
Copy-Item "$repo/packaging/licenses" "$stage/licenses" -Recurse
Compress-Archive -LiteralPath $stage -DestinationPath $output -CompressionLevel Optimal
Write-Output "Package: $output"
Write-Output "Staging: $stage"
Get-Item -LiteralPath $output | Select-Object Name,Length
