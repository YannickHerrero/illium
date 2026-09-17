# Opt-in Windows smoke test. Opens ONLY disposable Files/Tasks windows, one at
# a time, and changes ONLY their temporary configuration. No global keystrokes.
param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AppsOpacityTest {
 [StructLayout(LayoutKind.Sequential)] public struct Point {public int X,Y;}
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd,ref Point p);
}
'@
[void][AppsOpacityTest]::SetProcessDPIAware()
$root = Join-Path $env:TEMP ('winarchy-apps-opacity-test-' + [guid]::NewGuid())
$old = $env:WINARCHY_CONFIG_HOME
New-Item -ItemType Directory "$root\themes" -Force | Out-Null
$theme = (Get-Content "$PSScriptRoot/../config/themes/catppuccin-mocha.toml" -Raw).Replace('#1e1e2e', '#112233') + "`nbackground_opacity = 1.0`n"
function Save($path, $text) { [IO.File]::WriteAllText($path, $text) }
function Pixel($hwnd) {
    # Empty inset of the title/header, not text, selection or the WM focus ring.
    $point = New-Object AppsOpacityTest+Point
    $point.X=4; $point.Y=8
    [void][AppsOpacityTest]::ClientToScreen($hwnd, [ref]$point)
    $bitmap = New-Object Drawing.Bitmap 1,1
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try { $graphics.CopyFromScreen($point.X,$point.Y,0,0,$bitmap.Size); return $bitmap.GetPixel(0,0).ToArgb().ToString('X8') }
    finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Wait-Pixel($hwnd, $color, [bool]$equal=$true) {
    $until = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $actual = Pixel $hwnd
        if (($actual -eq $color) -eq $equal) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $until)
    throw "Pixel check failed: actual=$actual expected=$color equal=$equal (keep window unobscured)"
}
$p = $null
try {
    Save "$root\winarchy.toml" 'theme = "test"'
    Save "$root\themes\test.toml" $theme
    $env:WINARCHY_CONFIG_HOME = $root
    foreach ($app in @('files', 'tasks')) {
        $p = Start-Process $Executable -ArgumentList $app -PassThru
        $until = [DateTime]::UtcNow.AddSeconds(15)
        do { Start-Sleep -Milliseconds 100; $p.Refresh() } while ($p.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $until)
        $hwnd = $p.MainWindowHandle
        if ($hwnd -eq 0) { throw "No $app window" }
        Wait-Pixel $hwnd 'FF112233'
        Save "$root\background-opacity.state" "theme = 'test'`nopacity = 0.50`n"
        Wait-Pixel $hwnd 'FF112233' $false
        Remove-Item "$root\background-opacity.state"
        Wait-Pixel $hwnd 'FF112233'
        Save "$root\themes\test.toml" ($theme.Replace('#112233', '#332211'))
        Wait-Pixel $hwnd 'FF332211'
        Save "$root\themes\test.toml" ($theme.Replace('= 1.0', '= 2.0'))
        Start-Sleep -Milliseconds 300
        Wait-Pixel $hwnd 'FF332211'
        $p.Refresh()
        if ($p.MainWindowHandle -ne $hwnd) { throw "$app window was recreated" }
        [void]$p.CloseMainWindow()
        if (!$p.WaitForExit(5000)) { throw "$app did not exit" }
        $p = $null
        Save "$root\themes\test.toml" $theme
        Write-Host "PASS: $app live opacity, reset, palette reload, invalid theme retention, unchanged HWND"
    }
} finally {
    $env:WINARCHY_CONFIG_HOME = $old
    if ($p -and !$p.HasExited) { [void]$p.CloseMainWindow(); [void]$p.WaitForExit(5000) }
    Remove-Item -Recurse -Force $root
}
