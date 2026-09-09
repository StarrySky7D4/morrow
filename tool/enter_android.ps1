param(
    [string]$AndroidSdk = $env:ANDROID_HOME,
    [string]$Jdk = $env:JAVA_HOME
)
$ErrorActionPreference = 'Stop'
if (-not $Jdk) {
    $candidate = Get-ChildItem -LiteralPath "$PSScriptRoot/vendor/java" -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'bin/java.exe') } |
        Select-Object -First 1
    if ($candidate) { $Jdk = $candidate.FullName }
}
if (-not $Jdk -or -not (Test-Path -LiteralPath "$Jdk/bin/java.exe")) {
    throw 'JDK missing. Pass -Jdk <JDK 17 or newer directory>.'
}
if (-not $AndroidSdk) { $AndroidSdk = $env:ANDROID_SDK_ROOT }
if (-not $AndroidSdk) {
    $candidates = @("$PSScriptRoot/vendor/android-sdk")
    if ($env:LOCALAPPDATA) { $candidates += "$env:LOCALAPPDATA/Android/Sdk" }
    $AndroidSdk = $candidates | Where-Object { Test-Path -LiteralPath "$_/cmdline-tools/latest/bin/sdkmanager.bat" } | Select-Object -First 1
}
if (-not $AndroidSdk -or -not (Test-Path -LiteralPath $AndroidSdk)) {
    throw 'Android SDK missing. Run tool/setup_android.ps1 first.'
}
$env:JAVA_HOME = (Resolve-Path -LiteralPath $Jdk).Path
$env:ANDROID_HOME = (Resolve-Path -LiteralPath $AndroidSdk).Path
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$androidPaths = @("$env:JAVA_HOME/bin", "$env:ANDROID_HOME/platform-tools", "$env:ANDROID_HOME/cmdline-tools/latest/bin")
$env:PATH = (($androidPaths + ($env:PATH -split ';')) | Select-Object -Unique) -join ';'
Write-Output "JAVA_HOME=$env:JAVA_HOME"
Write-Output "ANDROID_HOME=$env:ANDROID_HOME"
