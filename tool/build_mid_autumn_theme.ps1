param([string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
function Get-ThemeFileSha256([string]$Path) {
    # MSBuild may launch Windows PowerShell with a restricted module search path.
    # Use the runtime directly so integrity checks do not require Get-FileHash.
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose(); $stream.Dispose() }
}
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot 'dist/plugins' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
Push-Location $projectRoot
try {
    $theme = Get-Content 'plugins/mid_autumn/theme.json' -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($theme.version -ne '1.0.2') { throw 'Update package_mid_autumn.rs version together with the theme manifest.' }
    $artwork = Join-Path $projectRoot 'plugins/mid_autumn/artwork/moonlit-garden.webp'
    if ((Get-Item -LiteralPath $artwork).Length -ne $theme.artwork.bytes -or
        (Get-ThemeFileSha256 $artwork) -ne $theme.artwork.sha256) {
        throw 'Artwork does not match the theme descriptor.'
    }
    cargo build --locked --release --manifest-path plugins/mid_autumn/Cargo.toml --target wasm32-unknown-unknown --target-dir build/mid-autumn-plugin
    if ($LASTEXITCODE -ne 0) { throw 'Theme guest build failed' }
    cargo build --locked --release --manifest-path workbench_host/Cargo.toml --target-dir build/workbench-host --example package_mid_autumn
    if ($LASTEXITCODE -ne 0) { throw 'Package builder failed' }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $destination = Join-Path $OutputDirectory "morrow-mid-autumn-$($theme.version).morrowplugin"
    & build/workbench-host/release/examples/package_mid_autumn.exe build/mid-autumn-plugin/wasm32-unknown-unknown/release/morrow_theme_mid_autumn.wasm $destination
    if ($LASTEXITCODE -ne 0) { throw 'Immutable theme packaging failed' }
    [pscustomobject]@{ Algorithm = 'SHA256'; Hash = Get-ThemeFileSha256 $destination; Path = $destination }
} finally { Pop-Location }
