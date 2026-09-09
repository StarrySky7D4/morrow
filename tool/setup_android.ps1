param(
    [string]$AndroidSdk = "$PSScriptRoot/vendor/android-sdk",
    [string]$Jdk = $env:JAVA_HOME,
    [uri]$Proxy
)
$ErrorActionPreference = 'Stop'
$sdkPath = [IO.Path]::GetFullPath($AndroidSdk)
$downloadDirectory = Join-Path $PSScriptRoot 'vendor/android-downloads'
$archive = Join-Path $downloadDirectory 'commandlinetools-win-15859902_latest.zip'
# Pinned to the Windows download and SHA-256 published at developer.android.com/studio.
$archiveUrl = 'https://dl.google.com/android/repository/commandlinetools-win-15859902_latest.zip'
$expectedHash = '90ae805d20434428bffcb699c290860f19bb5f66a67e6b330067e3de801fb04a'
$sdkManager = Join-Path $sdkPath 'cmdline-tools/latest/bin/sdkmanager.bat'
New-Item -ItemType Directory -Force -Path $sdkPath, $downloadDirectory | Out-Null
& "$PSScriptRoot/enter_android.ps1" -AndroidSdk $sdkPath -Jdk $Jdk
if ($Proxy -and ($Proxy.Scheme -ne 'http' -or $Proxy.UserInfo)) {
    throw 'Use an HTTP proxy without embedded credentials, for example http://127.0.0.1:10809.'
}
if (-not (Test-Path -LiteralPath $sdkManager)) {
    if (-not (Test-Path -LiteralPath $archive)) {
        $downloadArgs = @('--fail', '--location', '--show-error', '--connect-timeout', '15', '--max-time', '900', '--output', "$archive.partial")
        if ($Proxy) { $downloadArgs += @('--proxy', $Proxy.AbsoluteUri) }
        & curl.exe @downloadArgs $archiveUrl
        if ($LASTEXITCODE -ne 0) { throw "SDK download failed ($LASTEXITCODE). Check network access or pass -Proxy http://host:port." }
        $actualHash = (Get-FileHash -LiteralPath "$archive.partial" -Algorithm SHA256).Hash
        if ($actualHash -ne $expectedHash) { throw 'SDK archive checksum mismatch; it will not be extracted.' }
        Move-Item -LiteralPath "$archive.partial" -Destination $archive
    }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) {
        throw "SDK archive checksum mismatch: $archive"
    }
    $staging = Join-Path $downloadDirectory ([guid]::NewGuid().ToString())
    Expand-Archive -LiteralPath $archive -DestinationPath $staging
    $toolsParent = Join-Path $sdkPath 'cmdline-tools'
    New-Item -ItemType Directory -Force -Path $toolsParent | Out-Null
    $latest = Join-Path $toolsParent 'latest'
    if (Test-Path -LiteralPath $latest) { throw "Incomplete existing tools directory; inspect it before retrying: $latest" }
    $source = [IO.Path]::GetFullPath((Join-Path $staging 'cmdline-tools'))
    $destination = [IO.Path]::GetFullPath($latest)
    $downloadRoot = [IO.Path]::GetFullPath($downloadDirectory).TrimEnd('\') + '\'
    $sdkRoot = $sdkPath.TrimEnd('\') + '\'
    if (-not $source.StartsWith($downloadRoot, [StringComparison]::OrdinalIgnoreCase) -or
        -not $destination.StartsWith($sdkRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'SDK extraction paths are outside the selected installation directories.'
    }
    Move-Item -LiteralPath $source -Destination $destination
}
$managerArgs = @("--sdk_root=$sdkPath")
if ($Proxy) { $managerArgs += @('--proxy=http', "--proxy_host=$($Proxy.Host)", "--proxy_port=$($Proxy.Port)") }
& $sdkManager @managerArgs --version
if ($LASTEXITCODE -ne 0) { throw 'Android command-line tools could not start. Check the reported Java requirement.' }
# Interactive: sdkmanager displays the applicable licenses and requests acceptance.
& $sdkManager @managerArgs 'platform-tools' 'platforms;android-36' 'build-tools;36.0.0' 'ndk;28.2.13676358' 'cmake;3.22.1'
if ($LASTEXITCODE -ne 0) { throw "SDK package installation failed ($LASTEXITCODE)." }
$requiredFiles = @('platform-tools/adb.exe', 'platforms/android-36/android.jar', 'build-tools/36.0.0/aapt2.exe', 'ndk/28.2.13676358/source.properties', 'cmake/3.22.1/bin/cmake.exe')
foreach ($relative in $requiredFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $sdkPath $relative))) { throw "SDK installation incomplete: $relative" }
}
& flutter config --android-sdk $sdkPath --jdk-dir $env:JAVA_HOME
if ($LASTEXITCODE -ne 0) { throw 'Flutter SDK/JDK configuration failed.' }
& flutter doctor -v
if ($LASTEXITCODE -ne 0) { throw 'Flutter doctor failed; inspect the output above.' }
Write-Output 'SDK packages installed. Run tool/build_android.ps1 to verify an APK build.'
