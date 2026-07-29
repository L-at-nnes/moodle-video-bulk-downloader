#requires -version 5.1
<#
Launches the GUI and captures its window content directly via PrintWindow
(not a screen-region grab, so nothing else on the desktop can end up in the
image even if another window overlaps it) into docs/screenshot.png, then
closes the app. Used once to illustrate the README.
#>
param(
    [string]$ExePath
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $ExePath) { $ExePath = Join-Path $root "MVBD-Portable\mvbd-gui.exe" }
$outPath = Join-Path $root "docs\screenshot.png"

if (-not (Test-Path $ExePath)) {
    throw "GUI executable not found at $ExePath - run scripts/build-portable.ps1 first (or pass -ExePath)"
}

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Capture {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

# Without this, GetWindowRect returns DPI-virtualized (96dpi-scaled) coordinates
# while PrintWindow renders actual device pixels, so the bitmap ends up smaller
# than the real window and the capture gets cropped on scaled displays.
[Win32Capture]::SetProcessDPIAware() | Out-Null

$proc = Start-Process -FilePath $ExePath -PassThru
try {
    $hwnd = [IntPtr]::Zero
    for ($i = 0; $i -lt 100 -and $hwnd -eq [IntPtr]::Zero; $i++) {
        Start-Sleep -Milliseconds 200
        $proc.Refresh()
        $hwnd = $proc.MainWindowHandle
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw "GUI window never appeared" }

    [Win32Capture]::ShowWindow($hwnd, 9) | Out-Null # SW_RESTORE, in case it opened minimized
    Start-Sleep -Seconds 3 # let the webview finish its first paint

    $rect = New-Object Win32Capture+RECT
    [Win32Capture]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top

    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    [Win32Capture]::PrintWindow($hwnd, $hdc, 2) | Out-Null # PW_RENDERFULLCONTENT
    $graphics.ReleaseHdc($hdc)

    New-Item -ItemType Directory -Force -Path (Split-Path $outPath) | Out-Null
    $bitmap.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)

    $graphics.Dispose()
    $bitmap.Dispose()

    Write-Host "Saved $outPath" -ForegroundColor Green
} finally {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}
