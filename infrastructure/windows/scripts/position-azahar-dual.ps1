param(
  [string]$TopTitle = "Azahar",
  [string]$BottomTitle = "Azahar"
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
}
"@

$wins = Get-Process | Where-Object {
  $_.MainWindowHandle -ne 0 -and (
    $_.MainWindowTitle -like "*$TopTitle*" -or $_.MainWindowTitle -like "*$BottomTitle*"
  )
} | Sort-Object StartTime
if ($wins.Count -lt 2) {
  Write-Host "Need at least 2 Azahar windows; found $($wins.Count)"
  exit 1
}

$screenAll = [System.Windows.Forms.Screen]::AllScreens
if ($screenAll.Count -lt 2) {
  Write-Host "Need 2 displays; found $($screenAll.Count)"
  exit 1
}

$topWin = $wins[0]
$bottomWin = $wins[1]
$screenA = $screenAll[0].Bounds
$screenB = $screenAll[1].Bounds

# SWP_NOZORDER|SWP_SHOWWINDOW
$flags = 0x0040 -bor 0x0004
[Win32]::SetWindowPos($topWin.MainWindowHandle, [IntPtr]::Zero, $screenA.X, $screenA.Y, $screenA.Width, $screenA.Height, $flags) | Out-Null
[Win32]::SetWindowPos($bottomWin.MainWindowHandle, [IntPtr]::Zero, $screenB.X, $screenB.Y, $screenB.Width, $screenB.Height, $flags) | Out-Null
Write-Host "Placed top/bottom windows across first two displays"
