# Non-keyboard multi-tab smoke test. Uses only its own standalone process and
# disposable profile/config. Start tests/browser on a local HTTP server first.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [string]$TestUrl = 'http://127.0.0.1:8765/tabs.html?auto=1'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TabWindows {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [DllImport("user32.dll", EntryPoint="FindWindowExW", CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
}
'@
[void][TabWindows]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $until = [DateTime]::UtcNow.AddSeconds(30)
    while (!( & $Condition )) {
        if ($p.HasExited -or [DateTime]::UtcNow -gt $until) { throw "$Message. Logs: $root" }
        Start-Sleep -Milliseconds 100
    }
}
$root = Join-Path $env:TEMP ('winarchy-tabs-test-' + [guid]::NewGuid())
$config = Join-Path $root 'config'
New-Item -ItemType Directory -Force (Join-Path $config 'browser'), (Join-Path $config 'themes') | Out-Null
$origin = ([Uri]$TestUrl).GetLeftPart([UriPartial]::Authority)
$rules = (Get-Content "$PSScriptRoot/../tests/browser/custom.txt" -Raw).Replace('http://127.0.0.1:8765', $origin)
[IO.File]::WriteAllText((Join-Path $config 'browser/custom.txt'), $rules)
Copy-Item "$PSScriptRoot/../config/themes/catppuccin-mocha.toml" (Join-Path $config 'themes/test.toml')
[IO.File]::WriteAllText((Join-Path $config 'winarchy.toml'), 'theme = "test"')
$oldHome = $env:WINARCHY_CONFIG_HOME
$oldLocal = $env:LOCALAPPDATA
$p = $null
try {
    $env:WINARCHY_CONFIG_HOME = $config
    $env:LOCALAPPDATA = Join-Path $root 'local'
    $log = Join-Path $root 'startup.log'
    $p = Start-Process -FilePath $Exe -ArgumentList @('--standalone', $TestUrl) -RedirectStandardError $log -PassThru
    $handle = $p.Handle # retain handle for ExitCode in Windows PowerShell
    Wait-For { $p.Refresh(); $p.MainWindowTitle -match 'fixture.*first' } 'First tab did not load'
    $window = $p.MainWindowHandle
    $automation = [Windows.Automation.AutomationElement]::FromHandle($window)
    $buttonCondition = New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty,'Open second tab with window.open')
    Wait-For { $script:openTab = $automation.FindFirst([Windows.Automation.TreeScope]::Descendants,$buttonCondition); $null -ne $script:openTab } 'Fixture button unavailable'
    # The ?auto=1 attempt has no gesture and must not create an unsolicited tab.
    Start-Sleep -Milliseconds 300
    $first = [TabWindows]::FindWindowEx($window,[IntPtr]::Zero,'Chrome_WidgetWin_0',$null)
    if ([TabWindows]::FindWindowEx($window,$first,'Chrome_WidgetWin_0',$null) -ne [IntPtr]::Zero) { throw 'Unsolicited popup allocated another controller' }
    # Invoke only our own fixture button through accessibility, never desktop
    # keyboard/mouse injection. This supplies a genuine user activation.
    $script:openTab.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-For { $p.Refresh(); $p.MainWindowTitle -match 'fixture.*second' } 'Second tab did not become active'
    $views = @()
    $after = [IntPtr]::Zero
    while ($true) {
        $after = [TabWindows]::FindWindowEx($window, $after, 'Chrome_WidgetWin_0', $null)
        if ($after -eq [IntPtr]::Zero) { break }
        $views += $after
    }
    if ($views.Count -ne 2) { throw "Expected two retained WebViews, got $($views.Count)" }
    $visible = @($views | Where-Object { [TabWindows]::IsWindowVisible($_) })
    if ($visible.Count -ne 1) { throw "Expected exactly one visible tab, got $($visible.Count)" }
    $outer = New-Object TabWindows+Rect
    $inner = New-Object TabWindows+Rect
    [void][TabWindows]::GetWindowRect($window, [ref]$outer)
    [void][TabWindows]::GetWindowRect($visible[0], [ref]$inner)
    if ($outer.left -ne $inner.left -or $outer.top -ne $inner.top -or $outer.right -ne $inner.right -or $outer.bottom -ne $inner.bottom) { throw 'Active tab does not occupy the whole client area' }
    $library = Join-Path $config 'browser/library.json'
    Wait-For {
        $visits = @((Get-Content $library -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json).history)
        ($visits.title -match 'fixture.*first') -and ($visits.title -match 'fixture.*second')
    } 'Both tab navigations were not recorded independently'
    [void]$p.CloseMainWindow()
    if (!$p.WaitForExit(10000)) { throw 'Multi-tab browser did not close' }
    if ($p.ExitCode -ne 0) { throw "Browser exit code $($p.ExitCode)" }
    $output = Get-Content $log -Raw
    if ($output -match 'panicked') { throw 'Browser panicked' }
    $blocked = [regex]::Match($output, 'metric blocked_requests=(\d+)')
    if (!$blocked.Success -or [int]$blocked.Groups[1].Value -lt 2) { throw 'Both tabs did not block their fixture ad requests' }
    Write-Host "PASS: popup opens a second tab, both controllers retained, only active view visible, full page bounds, per-tab history/filtering, clean shutdown. Logs: $root"
} finally {
    if ($p -and !$p.HasExited) { [void]$p.CloseMainWindow() }
    $env:WINARCHY_CONFIG_HOME = $oldHome
    $env:LOCALAPPDATA = $oldLocal
}
