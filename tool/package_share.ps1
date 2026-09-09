param(
  [string]$CrtDirectory = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Redist\MSVC\14.44.35112\x64\Microsoft.VC143.CRT',
  [switch]$IncludeWeb
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$version = [regex]::Match((Get-Content -Raw "$repo/pubspec.yaml"), '(?m)^version:\s*([0-9.]+)').Groups[1].Value
$build = Join-Path $repo 'build/windows/x64/runner/Release'
$output = Join-Path $repo "dist/morrow-$version-windows-x64.zip"
if (Test-Path $output) { throw "Already exists: $output" }
$webOutput = Join-Path $repo "dist/morrow-$version-web.zip"
if ($IncludeWeb) {
  if (Test-Path $webOutput) { throw "Already exists: $webOutput" }
  if (!(Test-Path "$repo/build/web/main.dart.js")) { throw 'Build the Web release first.' }
}
foreach ($file in @('LICENSE', 'NOTICE')) {
  if (!(Test-Path "$repo/$file")) { throw "Missing license file: $file" }
}
foreach ($file in @("$build/morrow_studio.exe", "$build/libmpv-2.dll", "$build/data/app.so", "$CrtDirectory/vcruntime140.dll")) {
  if (!(Test-Path -LiteralPath $file)) { throw "Missing dependency: $file" }
}
$stage = Join-Path $repo ('build/share-' + [guid]::NewGuid().ToString('N') + '/morrow')
New-Item -ItemType Directory -Force $stage, "$repo/dist" | Out-Null
# Incremental builds can leave the previous product executable behind.
Get-ChildItem -LiteralPath $build |
  Where-Object { $_.Name -ne 'daemon_studio.exe' } |
  ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $stage -Recurse }
Copy-Item "$CrtDirectory/*.dll" $stage
Copy-Item "$repo/packaging/USER_GUIDE.txt" "$stage/使用说明.txt"
Copy-Item "$repo/packaging/THIRD_PARTY_NOTICES.txt" $stage
Copy-Item "$repo/packaging/licenses" "$stage/licenses" -Recurse
Copy-Item "$repo/LICENSE", "$repo/NOTICE" $stage
Compress-Archive -LiteralPath $stage -DestinationPath $output -CompressionLevel Optimal
Write-Output "Package: $output"
Write-Output "Staging: $stage"
Get-Item -LiteralPath $output | Select-Object Name,Length
$artifacts = @($output)
if ($IncludeWeb) {
  $webStage = Join-Path (Split-Path $stage -Parent) 'morrow-web'
  New-Item -ItemType Directory -Force $webStage | Out-Null
  Copy-Item "$repo/build/web/*" $webStage -Recurse
  Copy-Item "$repo/LICENSE", "$repo/NOTICE", "$repo/packaging/THIRD_PARTY_NOTICES.txt" $webStage
  Copy-Item "$repo/packaging/licenses" "$webStage/licenses" -Recurse
  Copy-Item "$repo/packaging/WEB_README.txt" "$webStage/使用说明.txt"
  Compress-Archive -LiteralPath $webStage -DestinationPath $webOutput -CompressionLevel Optimal
  $artifacts += $webOutput
  Write-Output "Web staging: $webStage"
  Get-Item -LiteralPath $webOutput | Select-Object Name,Length
}
$checksums = Join-Path $repo "dist/morrow-$version-SHA256SUMS.txt"
$artifacts | ForEach-Object {
  $digest = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
  "$digest  $(Split-Path $_ -Leaf)"
} | Set-Content -LiteralPath $checksums -Encoding utf8
Write-Output "Checksums: $checksums"
