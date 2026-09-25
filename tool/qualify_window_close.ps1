param(
  [Parameter(Mandatory=$true)][string]$Release,
  [Parameter(Mandatory=$true)][string]$Library,
  [Parameter(Mandatory=$true)][string]$Report,
  [int]$TestProcessId = 0
)
$ErrorActionPreference = 'Stop'
$releasePath = (Resolve-Path -LiteralPath $Release).Path
$libraryPath = (Resolve-Path -LiteralPath $Library).Path
$reportPath = [IO.Path]::GetFullPath($Report)
if (Test-Path -LiteralPath $reportPath) { throw 'Preserve existing qualification evidence.' }
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class CloseQualificationWindow {
  private delegate bool EnumProc(IntPtr window, IntPtr data);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr data);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, StringBuilder value, int count);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
  public static IntPtr Find(int process) {
    IntPtr result = IntPtr.Zero;
    EnumWindows((window, data) => {
      uint owner; GetWindowThreadProcessId(window, out owner);
      if (owner != process) return true;
      var name = new StringBuilder(128); GetClassName(window, name, name.Capacity);
      if (name.ToString() != "FLUTTER_RUNNER_WIN32_WINDOW") return true;
      result = window; return false;
    }, IntPtr.Zero);
    return result;
  }
}
'@
# This process belongs exclusively to this test. Never close another app window.
if ($TestProcessId) {
  $owned = Get-CimInstance Win32_Process -Filter "ProcessId = $TestProcessId"
  if ($owned.ExecutablePath -ne (Join-Path $releasePath 'morrow_studio.exe') -or
      -not $owned.CommandLine.Contains($libraryPath)) { throw 'Not this qualification process.' }
  $process = Get-Process -Id $TestProcessId
} else {
  $process = Start-Process -FilePath (Join-Path $releasePath 'morrow_studio.exe') `
  -ArgumentList @('--data-directory="' + $libraryPath + '"', '--managed-library', '--locale=zh') `
  -WorkingDirectory $releasePath -WindowStyle Hidden -PassThru `
  -RedirectStandardOutput ($reportPath + '.stdout.log') `
  -RedirectStandardError ($reportPath + '.stderr.log')
}
$null = $process.Handle # Retain the exit code even after Windows removes the PID.
$ready = [Diagnostics.Stopwatch]::StartNew()
do {
  Start-Sleep -Milliseconds 25
  $process.Refresh()
  if ($process.HasExited) { throw "Test application exited before close: $($process.ExitCode)" }
  $window = [CloseQualificationWindow]::Find($process.Id)
} while ($window -eq [IntPtr]::Zero -and $ready.ElapsedMilliseconds -lt 15000)
if ($window -eq [IntPtr]::Zero) { throw "No test window; PID $($process.Id)" }
# Allow the verified local startup to render before sending the real WM_CLOSE.
Start-Sleep -Milliseconds 1500
$children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($process.Id)" |
  Where-Object { $_.Name -eq 'morrow-workbench-host.exe' } |
  ForEach-Object { Get-Process -Id $_.ProcessId })
if ($children.Count -ne 1) { throw 'Expected exactly one owned content service.' }
$children | ForEach-Object { $null = $_.Handle }
$clock = [Diagnostics.Stopwatch]::StartNew()
$wasVisible = [CloseQualificationWindow]::IsWindowVisible($window)
if (-not [CloseQualificationWindow]::PostMessage($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
  throw 'Owned window did not accept WM_CLOSE.'
}
do {
  Start-Sleep -Milliseconds 5
  $process.Refresh()
} while (-not $process.HasExited -and [CloseQualificationWindow]::IsWindowVisible($window) -and $clock.ElapsedMilliseconds -lt 3000)
$hiddenMs = $clock.ElapsedMilliseconds
$hostExitMs = $null
do {
  if ($null -eq $hostExitMs -and @($children | ForEach-Object { $_.Refresh(); $_.HasExited }) -notcontains $false) {
    $hostExitMs = $clock.ElapsedMilliseconds
  }
  Start-Sleep -Milliseconds 5
  $process.Refresh()
} while (-not $process.HasExited -and $clock.ElapsedMilliseconds -lt 15000)
if (-not $process.HasExited) { throw "Close did not finish; owned PID $($process.Id) remains observable." }
$exitMs = $clock.ElapsedMilliseconds
$process.Refresh()
$hostExited = @($children | ForEach-Object { $_.Refresh(); $_.HasExited }) -notcontains $false
$result = [ordered]@{
  uiPid = $process.Id; hostPid = @($children | ForEach-Object { $_.Id })
  windowHiddenOrExitedMs = $hiddenMs; applicationExitedMs = $exitMs
  initiallyVisible = $wasVisible
  contentHostExitedMs = $hostExitMs
  applicationExitCode = $process.ExitCode; contentHostExited = $hostExited
  library = $libraryPath
}
$result | ConvertTo-Json | Set-Content -LiteralPath $reportPath -Encoding utf8
if ($process.ExitCode -ne 0 -or -not $hostExited -or $hiddenMs -ge 3000) { throw 'Window/service close qualification failed.' }
if (Select-String -LiteralPath ($reportPath + '.stderr.log') -Pattern 'AXTree|\[ERROR:flutter/' -Quiet) {
  throw 'Flutter error while closing.'
}
Get-Content -LiteralPath $reportPath
