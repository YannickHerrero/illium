# Measures native window discovery, NOT first presentation or shell readiness.
# Opens and closes only the windows created by this probe. Does not stop WSL.
param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [string]$Arguments = '',
    [int]$Count = 10,
    [int]$TimeoutMs = 10000,
    [string]$WindowClass = '',
    [string]$Output = ''
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class TerminalProbe {
    public delegate bool Callback(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb, IntPtr data);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder name, int count);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    public static Dictionary<long,uint> Windows(string cls) {
        var result = new Dictionary<long,uint>();
        EnumWindows((window, data) => {
            var name = new StringBuilder(256);
            GetClassName(window, name, name.Capacity);
            uint pid; GetWindowThreadProcessId(window, out pid);
            if (IsWindowVisible(window) && (cls.Length == 0 || name.ToString() == cls)) result[window.ToInt64()] = pid;
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
'@
if ($Count -lt 1 -or $Count -gt 100) { throw 'Count must be 1..100' }
$rows = @()
for ($i = 0; $i -lt $Count; $i++) {
    $before = [TerminalProbe]::Windows($WindowClass)
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $start = @{FilePath=$Executable; PassThru=$true}
    if ($Arguments) { $start.ArgumentList = $Arguments }
    $process = Start-Process @start
    $found = $null
    do {
        $windows = [TerminalProbe]::Windows($WindowClass)
        foreach ($entry in $windows.GetEnumerator()) {
            if (-not $before.ContainsKey($entry.Key) -and ($WindowClass -ne '' -or $entry.Value -eq $process.Id)) {
                $found = $entry; break
            }
        }
        if ($null -eq $found) { Start-Sleep -Milliseconds 1 }
    } while ($null -eq $found -and $clock.ElapsedMilliseconds -lt $TimeoutMs)
    $elapsed = $clock.Elapsed.TotalMilliseconds
    if ($null -eq $found) { throw "No new window within ${TimeoutMs}ms (use WindowClass for a resident/child process)" }
    Start-Sleep -Milliseconds 500
    $owner = Get-Process -Id $found.Value -ErrorAction SilentlyContinue
    $rows += [pscustomobject]@{
        iteration=$i+1; visible_ms=[math]::Round($elapsed,2)
        pid=$found.Value; private_mib=[math]::Round($owner.PrivateMemorySize64 / 1MB,2)
        working_set_mib=[math]::Round($owner.WorkingSet64 / 1MB,2)
    }
    [void][TerminalProbe]::PostMessage([IntPtr]$found.Key, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
}
$rows | Format-Table
$sorted = @($rows.visible_ms | Sort-Object)
[pscustomobject]@{
    count=$Count; p50_ms=$sorted[[math]::Ceiling($Count*0.50)-1]
    p95_ms=$sorted[[math]::Ceiling($Count*0.95)-1]
} | Format-List
if ($Output) { $rows | ConvertTo-Json | Set-Content -Encoding UTF8 $Output }
