param(
    [string]$AndroidSdk = $env:ANDROID_HOME,
    [string]$Jdk = $env:JAVA_HOME,
    [ValidateSet('android-arm64', 'android-arm', 'android-x64')]
    [string]$Architecture = 'android-arm64',
    [switch]$Release
)
$ErrorActionPreference = 'Stop'
$projectDirectory = Split-Path $PSScriptRoot -Parent
$previousJavaToolOptions = $env:JAVA_TOOL_OPTIONS
Push-Location $projectDirectory
try {
    if (-not $Jdk) {
        $bundledJdk = Get-ChildItem -LiteralPath 'tool/vendor/java' -Directory -ErrorAction SilentlyContinue |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'bin/java.exe') } |
            Select-Object -First 1
        if ($bundledJdk) { $Jdk = $bundledJdk.FullName }
    }
    if ($Jdk) {
        $env:JAVA_HOME = (Resolve-Path -LiteralPath $Jdk).Path
        $env:PATH = "$env:JAVA_HOME\bin;$env:PATH"
    }
    if (-not $AndroidSdk) { $AndroidSdk = $env:ANDROID_SDK_ROOT }
    if (-not $AndroidSdk -and (Test-Path -LiteralPath "$PSScriptRoot/vendor/android-sdk/platform-tools/adb.exe")) {
        $AndroidSdk = "$PSScriptRoot/vendor/android-sdk"
    }
    if (-not $AndroidSdk -and $env:LOCALAPPDATA) {
        $AndroidSdk = Join-Path $env:LOCALAPPDATA 'Android/Sdk'
    }
    if (-not $AndroidSdk -or -not (Test-Path -LiteralPath $AndroidSdk)) {
        throw 'Android SDK missing. Install SDK 36 and pass -AndroidSdk <path>. See docs/ANDROID.md.'
    }
    $env:ANDROID_HOME = (Resolve-Path -LiteralPath $AndroidSdk).Path
    $env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
    if ($Release -and -not (Test-Path -LiteralPath 'android/key.properties')) {
        throw 'Release signing requires android/key.properties. Omit -Release for a debug preview.'
    }
    # Avoid the Windows AF_UNIX failure observed with the default Java temp path.
    # JAVA_TOOL_OPTIONS reaches both the Gradle client and its daemon process.
    if ($env:JAVA_TOOL_OPTIONS -notmatch 'jdk\.net\.unixdomain\.tmpdir=') {
        $javaSocketDirectory = Join-Path $projectDirectory 'build/android-env-check'
        New-Item -ItemType Directory -Force -Path $javaSocketDirectory | Out-Null
        $socketOption = '-Djdk.net.unixdomain.tmpdir="' + $javaSocketDirectory + '"'
        $env:JAVA_TOOL_OPTIONS = ($env:JAVA_TOOL_OPTIONS + ' ' + $socketOption).Trim()
    }
    $mode = if ($Release) { 'release' } else { 'debug' }
    & flutter build apk "--$mode" --target-platform $Architecture --split-per-abi
    if ($LASTEXITCODE -ne 0) { throw "Android build failed ($LASTEXITCODE)." }
    $versionLine = Get-Content -LiteralPath pubspec.yaml | Where-Object { $_ -match '^version:' }
    $version = (($versionLine -split ':', 2)[1].Trim() -split '\+')[0]
    New-Item -ItemType Directory -Force dist | Out-Null
    $destination = "dist/morrow-$version-$Architecture-$mode.apk"
    $abi = @{'android-arm64'='arm64-v8a'; 'android-arm'='armeabi-v7a'; 'android-x64'='x86_64'}[$Architecture]
    Copy-Item -LiteralPath "build/app/outputs/flutter-apk/app-$abi-$mode.apk" -Destination $destination
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
    "$hash  $(Split-Path $destination -Leaf)" | Set-Content -Encoding ascii -LiteralPath "$destination.sha256"
    Write-Output "APK: $destination"
    Write-Output "SHA256: $hash"
} finally {
    $env:JAVA_TOOL_OPTIONS = $previousJavaToolOptions
    Pop-Location
}
