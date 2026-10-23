param(
    [string]$OutputDirectory = '',
    [string]$RetainedPackage = $env:MORROW_RETAINED_THEME_PACKAGE,
    [string]$RetainedSha256 = $env:MORROW_RETAINED_THEME_SHA256
)
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
    if ($theme.version -ne '1.0.3') { throw 'Update package_mid_autumn.rs version together with the theme manifest.' }
    $destination = Join-Path $OutputDirectory "morrow-mid-autumn-$($theme.version).morrowplugin"
    $retainedBytes = $null
    if ($RetainedPackage -or $RetainedSha256) {
        if (-not $RetainedPackage -or $RetainedSha256 -notmatch '^[a-fA-F0-9]{64}$') {
            throw 'Retained theme package requires a path and exact SHA-256'
        }
        # Retention is for a reviewed application build, never a replacement of
        # an immutable release under the same version in dist/plugins.
        $cacheBundle = [IO.Path]::GetFullPath((Join-Path $projectRoot 'build/mid-autumn-plugin/bundle'))
        if ($OutputDirectory -ne $cacheBundle) {
            throw 'Retained theme is only supported in the application build cache'
        }
        $retainedBytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $RetainedPackage).Path)
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $actual = ([BitConverter]::ToString($sha.ComputeHash($retainedBytes))).Replace('-', '').ToLowerInvariant() }
        finally { $sha.Dispose() }
        if ($actual -ne $RetainedSha256.ToLowerInvariant()) { throw 'Retained theme package SHA-256 mismatch' }
        if ((Test-Path -LiteralPath $destination) -and (Get-ThemeFileSha256 $destination) -ne $actual) {
            throw 'Existing theme bundle differs from retained package'
        }
    }
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
    if ($null -ne $retainedBytes) {
        [IO.File]::WriteAllBytes($destination, $retainedBytes)
        Write-Output "Retained theme package SHA-256 verified: $actual (new guest compiled separately)"
    } else {
        & build/workbench-host/release/examples/package_mid_autumn.exe build/mid-autumn-plugin/wasm32-unknown-unknown/release/morrow_theme_mid_autumn.wasm $destination
        if ($LASTEXITCODE -ne 0) { throw 'Immutable theme packaging failed' }
    }
    [pscustomobject]@{ Algorithm = 'SHA256'; Hash = Get-ThemeFileSha256 $destination; Path = $destination }
} finally { Pop-Location }
