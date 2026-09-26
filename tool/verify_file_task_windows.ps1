# Rebuild the file-read fixture from the pinned compatibility guest, then run
# the Dart -> real Windows host test. The caller supplies an existing builtin
# workbench package compatible with the host under review.
# Example:
#   powershell -File tool/verify_file_task_windows.ps1 -WorkbenchPackage C:\path\to\workbench.morrowplugin
param(
    [Parameter(Mandatory = $true)]
    [string]$WorkbenchPackage,
    [string]$HostExecutable = 'build/workbench-host/release/morrow-workbench-host.exe'
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') {
    throw 'This verification requires Windows.'
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$logRoot = Join-Path $repoRoot 'build/local-review'
$runId = [guid]::NewGuid().ToString('N').Substring(0, 8)
$fixtureDir = Join-Path $logRoot "file-task-fixture-$runId"
$fixture = Join-Path $fixtureDir 'file-read.mplugin'
$packageLog = Join-Path $logRoot "file-task-fixture-$runId.log"
$testLog = Join-Path $logRoot "file-task-real-native-$runId.log"
$savedHost = $env:MORROW_WORKBENCH_HOST
$savedPackage = $env:MORROW_WORKBENCH_PACKAGE
$savedFixture = $env:MORROW_FILE_TASK_PACKAGE

Push-Location $repoRoot
try {
    $hostPath = (Resolve-Path -LiteralPath $HostExecutable).Path
    $builtinPath = (Resolve-Path -LiteralPath $WorkbenchPackage).Path
    $guestPath = (Resolve-Path -LiteralPath 'sdk/compat/transport-v1-rc1/rust-io.wasm').Path
    [System.IO.Directory]::CreateDirectory($fixtureDir) | Out-Null

    $packArgs = @(
        'run', '--locked', '--offline',
        '--manifest-path', 'core/Cargo.toml',
        '--target-dir', 'build/workbench-host',
        '--release', '--example', 'plugin_package', '--',
        'pack-v2', $guestPath, $fixture,
        'org.example.workbench.file', '1.0.0',
        '--io-capability', 'file-read',
        '--io-handler', 'file.read-selected',
        '--io-resources', '2'
    )
    # Windows PowerShell 5 turns Cargo's normal stderr progress into a
    # NativeCommandError when Stop is active. Judge native exit codes directly.
    $ErrorActionPreference = 'Continue'
    & cargo @packArgs *> $packageLog
    $packageExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($packageExit -ne 0) {
        Get-Content -LiteralPath $packageLog -Tail 30
        throw "File-read fixture build failed with exit code $packageExit"
    }

    $env:MORROW_WORKBENCH_HOST = $hostPath
    $env:MORROW_WORKBENCH_PACKAGE = $builtinPath
    $env:MORROW_FILE_TASK_PACKAGE = $fixture
    $ErrorActionPreference = 'Continue'
    & flutter test --no-pub test/file_task_real_native_test.dart *> $testLog
    $testExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($testExit -ne 0) {
        Get-Content -LiteralPath $testLog -Tail 40
        throw "Real Windows file-task test failed with exit code $testExit"
    }
    Get-Content -LiteralPath $testLog -Tail 5
    Write-Host "Fixture build log: $packageLog"
    Write-Host "Native test log: $testLog"
} finally {
    $env:MORROW_WORKBENCH_HOST = $savedHost
    $env:MORROW_WORKBENCH_PACKAGE = $savedPackage
    $env:MORROW_FILE_TASK_PACKAGE = $savedFixture
    $resolvedLogRoot = [System.IO.Path]::GetFullPath($logRoot)
    $resolvedFixtureDir = [System.IO.Path]::GetFullPath($fixtureDir)
    if ($resolvedFixtureDir.StartsWith(
        "$resolvedLogRoot$([System.IO.Path]::DirectorySeparatorChar)",
        [System.StringComparison]::OrdinalIgnoreCase
    ) -and [System.IO.Directory]::Exists($resolvedFixtureDir)) {
        Remove-Item -LiteralPath $resolvedFixtureDir -Recurse -Force
    }
    Pop-Location
}
