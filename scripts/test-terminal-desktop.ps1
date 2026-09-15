# Opt-in: opens one disposable WSL window, samples its pixels, changes ONLY
# its temporary configuration, then closes it. Never types into other windows.
param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TerminalDesktopTest {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int L,T,R,B;}
 [StructLayout(LayoutKind.Sequential)] public struct Point {public int X,Y;}
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,out Rect r);
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd,ref Point p);
}
'@
[void][TerminalDesktopTest]::SetProcessDPIAware()
$configHome=Join-Path $env:TEMP ('winarchy-terminal-test-'+[guid]::NewGuid())
$old=$env:WINARCHY_CONFIG_HOME
New-Item -ItemType Directory "$configHome\themes" -Force | Out-Null
$theme=@'
name = "Terminal test"
background = "#112233"
surface = "#313244"
overlay = "#45475a"
text = "#cdd6f4"
subtext = "#a6adc8"
accent = "#cba6f7"
green = "#a6e3a1"
yellow = "#f9e2af"
red = "#f38ba8"
terminal_background_opacity = 1.0
'@
function Save-Utf8($path,$text) {[IO.File]::WriteAllText($path,$text,(New-Object Text.UTF8Encoding $false))}
function Background($hwnd) {
 $r=New-Object TerminalDesktopTest+Rect
 [void][TerminalDesktopTest]::GetClientRect($hwnd,[ref]$r)
 $point=New-Object TerminalDesktopTest+Point
 $point.X=[int](($r.R-$r.L)/2);$point.Y=($r.B-$r.T)-20
 [void][TerminalDesktopTest]::ClientToScreen($hwnd,[ref]$point)
 $bitmap=New-Object Drawing.Bitmap 1,1
 $g=[Drawing.Graphics]::FromImage($bitmap)
 try {$g.CopyFromScreen($point.X,$point.Y,0,0,$bitmap.Size);return $bitmap.GetPixel(0,0).ToArgb().ToString('X8')}
 finally {$g.Dispose();$bitmap.Dispose()}
}
function Wait-Color($hwnd,$expected) {
 $clock=[Diagnostics.Stopwatch]::StartNew()
 do {$actual=Background $hwnd;if ($actual -eq $expected) {return};Start-Sleep -Milliseconds 50} while ($clock.ElapsedMilliseconds -lt 5000)
 throw "Expected $expected, got $actual (window must remain unobscured)"
}
$p=$null
try {
 Save-Utf8 "$configHome\winarchy.toml" 'theme = "test"'
 Save-Utf8 "$configHome\themes\test.toml" $theme
 Save-Utf8 "$configHome\terminal.toml" 'font_size = 14.0'
 $env:WINARCHY_CONFIG_HOME=$configHome
 $p=Start-Process $Executable -ArgumentList '--standalone' -PassThru
 Start-Sleep -Seconds 3;$p.Refresh();$hwnd=$p.MainWindowHandle
 if ($hwnd -eq 0) {throw 'No terminal window'}
 Wait-Color $hwnd 'FF112233'
 $children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($p.Id)" | Where-Object Name -eq 'wsl.exe' | Select-Object -ExpandProperty ProcessId)
 if ($children.Count -ne 1) {throw 'Expected exactly one WSL process'}
 Save-Utf8 "$configHome\themes\test.toml" ($theme.Replace('#112233','#332211'))
 Wait-Color $hwnd 'FF332211'
 Save-Utf8 "$configHome\terminal.toml" 'font_size = 18.0'
 Start-Sleep -Milliseconds 300
 Save-Utf8 "$configHome\themes\test.toml" ($theme.Replace('= 1.0','= 2.0'))
 Start-Sleep -Milliseconds 300
 Wait-Color $hwnd 'FF332211'
 $after=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($p.Id)" | Where-Object Name -eq 'wsl.exe' | Select-Object -ExpandProperty ProcessId)
 if (($children -join ',') -ne ($after -join ',')) {throw 'Reload restarted WSL'}
 $p.Refresh();if ($p.MainWindowHandle -ne $hwnd) {throw 'Reload replaced the window'}
 'PASS: live palette, font reload, invalid alpha retention, unchanged HWND and WSL PID'
} finally {
 $env:WINARCHY_CONFIG_HOME=$old
 if ($null -ne $p -and -not $p.HasExited) {[void]$p.CloseMainWindow();if (-not $p.WaitForExit(5000)) {throw 'Test terminal did not exit'}}
 Remove-Item -Recurse -Force $configHome
}
