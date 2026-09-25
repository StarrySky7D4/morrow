$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
  $destination = Join-Path $repo 'build/web'
  if (!(Test-Path "$destination/index.html")) { throw 'Build the Web application first' }
  $commit = git rev-parse HEAD
  if ($LASTEXITCODE -ne 0) { throw 'Cannot identify source commit' }
  Copy-Item LICENSE,NOTICE,packaging/THIRD_PARTY_NOTICES.txt $destination -Force
  New-Item -ItemType Directory -Force "$destination/licenses" | Out-Null
  Copy-Item packaging/licenses/* "$destination/licenses" -Recurse -Force
  $sysroot = rustc --print sysroot
  Copy-Item "$sysroot/share/doc/rust/COPYRIGHT-library.html" "$destination/licenses/" -Force
  $inventory = @{}
  foreach ($manifest in @('core-web/Cargo.toml','plugins/workbench/Cargo.toml')) {
    $metadata = cargo metadata --locked --format-version 1 --manifest-path $manifest
    if ($LASTEXITCODE -ne 0) { throw 'Cargo license inventory failed' }
    foreach ($package in ($metadata | ConvertFrom-Json).packages | Where-Object { $_.source }) {
      $key = "$($package.name)-$($package.version)"
      $folder = "$destination/licenses/cargo/$key"
      New-Item -ItemType Directory -Force $folder | Out-Null
      Get-ChildItem (Split-Path $package.manifest_path) -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)' } | Copy-Item -Destination $folder -Force
      $inventory[$key] = "$key : $($package.license) $($package.repository)"
    }
  }
  $inventory.Values | Sort-Object | Set-Content "$destination/licenses/cargo-dependencies.txt" -Encoding utf8
  "Morrow Web - AGPL-3.0-only`nCorresponding source: https://github.com/StarrySky7D4/morrow/tree/$commit`nBuild: tool/build_web_workbench.ps1 -BaseHref /morrow/`nLicense: LICENSE; third-party notices: THIRD_PARTY_NOTICES.txt and licenses/" | Set-Content "$destination/SOURCE.txt" -Encoding utf8
  @{commit=$commit;source="https://github.com/StarrySky7D4/morrow/tree/$commit";builtAt=[DateTime]::UtcNow.ToString('o');basePath='/morrow/'} | ConvertTo-Json | Set-Content "$destination/release.json" -Encoding utf8
  New-Item -ItemType File -Force "$destination/.nojekyll" | Out-Null
  Get-ChildItem $destination -Recurse -File | Where-Object { $_.Name -ne 'SHA256SUMS.txt' } | ForEach-Object {
    "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetRelativePath($destination,$_.FullName).Replace('\','/'))"
  } | Set-Content "$destination/SHA256SUMS.txt" -Encoding utf8
} finally { Pop-Location }
