# Run against an updated Winarchy on an EMPTY workspace. Never changes workspace,
# stops the daemon, modifies personal browser data, or closes unrelated windows.
param(
    [Parameter(Mandatory = $true)][string]$Bin,
    [switch]$KeepScene
)
$ErrorActionPreference = 'Stop'
$ctl = Join-Path $Bin 'winarchyctl.exe'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class DemoProbe {
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder text, int size);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr GetProp(IntPtr h, string name);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
function Status {
    $text = & $ctl status
    if ($LASTEXITCODE -ne 0) { throw 'Cannot query Winarchy status' }
    return ($text | ConvertFrom-Json)
}
function Owner([long]$id) {
    [uint32]$ownerId = 0
    [void][DemoProbe]::GetWindowThreadProcessId([IntPtr]$id, [ref]$ownerId)
    return $ownerId
}
function Class([long]$id) {
    $text = [Text.StringBuilder]::new(128)
    [void][DemoProbe]::GetClassName([IntPtr]$id, $text, 128)
    return $text.ToString()
}
function Assert($condition, [string]$message) {
    if (-not $condition) { throw $message }
}
$before = Status
Assert ($null -ne $before.demo_pending) 'Daemon does not support demo; update all binaries first'
Assert (-not $before.demo_pending) 'Another demo is starting'
Assert (@($before.clients | Where-Object workspace -eq $before.workspace).Count -eq 0) 'Use an empty workspace'
$owned = @()
try {
    & $ctl demo
    Assert ($LASTEXITCODE -eq 0) 'Demo command failed'
    $deadline = [DateTime]::UtcNow.AddSeconds(35)
    do {
        Start-Sleep -Milliseconds 150
        $state = Status
        Assert ($state.workspace -eq $before.workspace) 'Workspace changed during test'
        Assert (-not $state.demo_error) "Demo failed: $($state.demo_error)"
        Assert ([DateTime]::UtcNow -lt $deadline) 'Demo did not finish in time'
    } while ($state.demo_pending)
    $windows = @($state.clients | Where-Object workspace -eq $before.workspace)
    Assert ($windows.Count -eq 3) 'Expected exactly three demo windows'
    foreach ($window in $windows) {
        Assert ([DemoProbe]::GetProp([IntPtr]$window.id, 'WinarchyDemoReady') -ne [IntPtr]::Zero) 'Window is not a ready demo'
        $owned += [pscustomobject]@{ Id = [long]$window.id; Owner = (Owner $window.id) }
    }
    Assert ((Class $windows[0].id) -eq 'WinarchyTerminal') 'Left window must be a terminal'
    Assert ((Class $windows[1].id) -eq 'WinarchyTerminal') 'Top-right window must be a terminal'
    Assert ((Class $windows[2].id) -eq 'WinarchyBrowser') 'Bottom-right window must be the browser'
    $left, $top, $bottom = $windows[0].rect, $windows[1].rect, $windows[2].rect
    Assert ($left.x -lt $top.x -and $left.y -lt $bottom.y) 'Left/right geometry is wrong'
    Assert ([Math]::Abs($top.x - $bottom.x) -le 2 -and $top.y -lt $bottom.y) 'Right-hand split is wrong'
    Assert ([Math]::Abs($left.w - $top.w) -le 4) 'Width split must be 50/50'
    Assert ([Math]::Abs($top.h - $bottom.h) -le 4) 'Height split must be 50/50'
    Assert ($state.focused -eq $windows[0].id) 'Left terminal should be focused'
    # Reject a second scene, without modifying any of the first scene's windows.
    $oldPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue' # Expected native stderr on Windows PowerShell 5.
        & $ctl demo 2>$null
        $rejected = $LASTEXITCODE -ne 0
    } finally { $ErrorActionPreference = $oldPreference }
    Assert $rejected 'Occupied workspace was not rejected'
    $after = Status
    Assert (@($after.clients | Where-Object workspace -eq $before.workspace).Count -eq 3) 'Rejection changed the scene'
    Write-Host 'Demo startup, geometry, focus and occupied-workspace rejection passed.'
    Write-Host 'Inspect synthetic build/fetch/home and switch themes for visual/live-reload checks.'
} finally {
    if (-not $KeepScene) {
        foreach ($window in $owned) {
            if ((Owner $window.Id) -eq $window.Owner -and [DemoProbe]::GetProp([IntPtr]$window.Id, 'WinarchyDemoReady') -ne [IntPtr]::Zero) {
                [void][DemoProbe]::PostMessage([IntPtr]$window.Id, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
            }
        }
    }
}
