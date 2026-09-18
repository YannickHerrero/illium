# Explicit Windows desktop smoke test. Opens and closes ONLY its own browser process.
# Start the fixture first: python -m http.server 8765 --bind 127.0.0.1 --directory tests/browser
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [string]$TestUrl = 'http://127.0.0.1:8765/index.html',
    [switch]$CheckTilingFocus,
    # Allows palette/navigation checks when the desktop cannot grant foreground focus.
    [switch]$SkipFocusChecks,
    # Opt-in real keyboard input, restricted to this test's foreground window.
    # Do not use the keyboard/mouse while running this check.
    [switch]$CheckLeader
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BrowserTest {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int x, y; }
    [StructLayout(LayoutKind.Sequential)] public struct GuiThreadInfo {
        public uint cbSize, flags;
        public IntPtr active, focus, capture, menuOwner, moveSize, caret;
        public Rect caretRect;
    }
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint id, ref GuiThreadInfo info);
    [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr GetText(IntPtr h, uint m, IntPtr w, System.Text.StringBuilder text);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point p);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll", EntryPoint="FindWindowExW", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr SetText(IntPtr h, uint m, IntPtr w, string s);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr h, out uint key, out byte alpha, out uint flags);
}
'@
[void][BrowserTest]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $until = [DateTime]::UtcNow.AddSeconds(30)
    while (!( & $Condition )) {
        if ([DateTime]::UtcNow -gt $until) { throw $Message }
        Start-Sleep -Milliseconds 100
    }
}
$root = Join-Path $env:TEMP ('winarchy-browser-test-' + [guid]::NewGuid())
$configDir = Join-Path $root 'config'
$filters = Join-Path $configDir 'browser'
New-Item -ItemType Directory -Force $filters | Out-Null
Copy-Item "$PSScriptRoot/../tests/browser/custom.txt" $filters
New-Item -ItemType Directory -Force (Join-Path $configDir 'themes') | Out-Null
$themePath = Join-Path $configDir 'themes/test.toml'
$theme = (Get-Content "$PSScriptRoot/../config/themes/catppuccin-mocha.toml" -Raw) + "`nbackground_opacity = 0.75`n"
[IO.File]::WriteAllText($themePath, $theme)
[IO.File]::WriteAllText((Join-Path $configDir 'winarchy.toml'), 'theme = "test"')
$opacityPath = Join-Path $configDir 'background-opacity.state'
@{
    history = @(@{ url='https://rust-lang.org/'; title='Rust language' })
    bookmarks = @(@{ url='https://bookmarked.example/'; title='Bookmarked page' })
} | ConvertTo-Json -Depth 4 | Set-Content -Encoding Ascii (Join-Path $filters 'library.json')
$oldHome = $env:WINARCHY_CONFIG_HOME
$oldLocal = $env:LOCALAPPDATA
$process = $null
$log = Join-Path $root 'startup.log'
try {
    $env:WINARCHY_CONFIG_HOME = $configDir
    $env:LOCALAPPDATA = Join-Path $root 'local'
    $process = Start-Process -FilePath $Exe -ArgumentList '--standalone' -PassThru -RedirectStandardError $log
    Wait-For { $process.Refresh(); $process.MainWindowHandle -ne [IntPtr]::Zero } 'No browser window'
    Wait-For { (Get-Content $log -Raw -ErrorAction SilentlyContinue) -match 'webview_ready_ms=' } 'WebView not ready'
    $window = $process.MainWindowHandle
    if ($CheckTilingFocus) {
        # Requires Winarchy running; do not move the mouse during this check.
        [void][BrowserTest]::SetThreadDpiAwarenessContext([IntPtr](-4))
        Wait-For { [BrowserTest]::GetForegroundWindow() -eq $window } 'New browser did not receive foreground focus'
        $rect = New-Object BrowserTest+Rect
        $cursor = New-Object BrowserTest+Point
        [void][BrowserTest]::GetWindowRect($window, [ref]$rect)
        [void][BrowserTest]::GetCursorPos([ref]$cursor)
        if ([Math]::Abs($cursor.x - ($rect.left + $rect.right)/2) -gt 2 -or [Math]::Abs($cursor.y - ($rect.top + $rect.bottom)/2) -gt 2) {
            throw "Pointer not centered in the browser tile: actual=($($cursor.x),$($cursor.y)), expected=($(($rect.left+$rect.right)/2),$(($rect.top+$rect.bottom)/2))"
        }
        Write-Host 'PASS: new browser focused and pointer centered after tiling.'
    }
    $panel = [BrowserTest]::GetDlgItem($window, 104)
    $edit = [BrowserTest]::GetDlgItem($panel, 101)
    $list = [BrowserTest]::GetDlgItem($panel, 102)
    if (![BrowserTest]::IsWindowVisible($edit)) { throw 'Home input is not visible' }
    function Assert-CenteredPalette {
        $outer = New-Object BrowserTest+Rect
        $inner = New-Object BrowserTest+Rect
        [void][BrowserTest]::GetWindowRect($window, [ref]$outer)
        [void][BrowserTest]::GetWindowRect($panel, [ref]$inner)
        if ([Math]::Abs(($outer.left+$outer.right)-($inner.left+$inner.right)) -gt 2 -or
            [Math]::Abs(($outer.top+$outer.bottom)-($inner.top+$inner.bottom)) -gt 2) {
            throw 'Navigation palette is not centered'
        }
        if (($inner.right-$inner.left) -gt (680 * [BrowserTest]::GetDpiForWindow($window) / 96 + 1)) { throw 'Navigation palette exceeds compact maximum width' }
        if ($inner.left -lt $outer.left -or $inner.right -gt $outer.right -or
            $inner.top -lt $outer.top -or $inner.bottom -gt $outer.bottom) { throw 'Palette overflows the browser' }
    }
    Assert-CenteredPalette
    [uint32]$key=0; [byte]$alpha=0; [uint32]$flags=0
    if (![BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) -or $alpha -ne 191) { throw 'Home alpha did not use the theme 75%' }
    [IO.File]::WriteAllText($opacityPath, "theme = 'test'`nopacity = 0.60`n")
    Wait-For { [BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) -and $alpha -eq 153 } 'Home did not follow live 60% override'
    [IO.File]::WriteAllText($themePath, $theme.Replace('= 0.75', '= 2.0'))
    Start-Sleep -Milliseconds 300
    if (![BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) -or $alpha -ne 153) { throw 'Invalid theme changed the last valid opacity' }
    [IO.File]::WriteAllText($themePath, $theme)
    if (!$SkipFocusChecks) {
    [void][BrowserTest]::SetForegroundWindow($window)
    Wait-For { [BrowserTest]::GetForegroundWindow() -eq $window } 'Desktop did not grant foreground focus; use -SkipFocusChecks on an unattended desktop'
    # Select a suggestion without clicking (click now opens it), then route a
    # character addressed to the list into the native search field.
    [void][BrowserTest]::SendMessage($list,0x0186,[IntPtr]::Zero,[IntPtr]::Zero)
    $thread = [BrowserTest]::GetWindowThreadProcessId($window,[IntPtr]::Zero)
    $gui = New-Object BrowserTest+GuiThreadInfo
    $gui.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($gui)
    # Deliver a translated character, independent of modifiers the user may be
    # holding on the live desktop. Never synthesize global key releases here.
    [void][BrowserTest]::PostMessage($list,0x0102,[IntPtr]0x61,[IntPtr]1)
    Wait-For { [void][BrowserTest]::GetGUIThreadInfo($thread,[ref]$gui); $gui.focus -eq $edit } 'Typing did not return focus to search'
    $text = New-Object Text.StringBuilder 256
    Wait-For { [void][BrowserTest]::GetText($edit,0x000D,[IntPtr]256,$text); $text.Length -eq 1 } 'First typed character was lost or duplicated'
    # Queued Unicode input targeting the home container is forwarded as well.
    [void][BrowserTest]::PostMessage($window,0x0102,[IntPtr]0x00E9,[IntPtr]1)
    Wait-For { [void][BrowserTest]::GetText($edit,0x000D,[IntPtr]256,$text); $text.ToString().EndsWith([string][char]0x00E9) } 'Unicode input was lost'
    }
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, 'bkmd')
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 1 } 'Fuzzy bookmark match missing'
    if ([BrowserTest]::SendMessage($list,0x0188,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -ne -1) { throw 'Typing automatically selected a result' }
    [void][BrowserTest]::PostMessage($edit,0x0100,[IntPtr]40,[IntPtr]::Zero)
    Wait-For { [BrowserTest]::SendMessage($list,0x0188,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 0 } 'Down did not select the first result'
    [void][BrowserTest]::PostMessage($edit,0x0100,[IntPtr]27,[IntPtr]::Zero)
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 2 } 'Escape on home did not clear the query'
    if (![BrowserTest]::IsWindowVisible($panel)) { throw 'Escape closed the home palette' }
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, 'zzzzzzzz')
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 0 } 'Unrelated suggestion was retained'
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, $TestUrl)
    Start-Sleep -Milliseconds 200
    [void][BrowserTest]::PostMessage($edit,0x0100,[IntPtr]13,[IntPtr]::Zero)
    $library = Join-Path $filters 'library.json'
    Wait-For { ((Get-Content $library -Raw | ConvertFrom-Json).history.url) -contains $TestUrl } 'Successful visit was not recorded'
    if ([BrowserTest]::IsWindowVisible($edit)) { throw 'Home input still visible on the page' }
    if ([BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags)) { throw 'Page should no longer be layered/translucent' }
    [IO.File]::WriteAllText($opacityPath, "theme = 'test'`nopacity = 0.45`n")
    Start-Sleep -Milliseconds 400
    if ([BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags)) { throw 'Live opacity made a web page translucent' }
    # Invoke the same queued action as Ctrl+D without injecting keys into the user's desktop.
    [void][BrowserTest]::PostMessage($window,0x8005,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-For { ((Get-Content $library -Raw | ConvertFrom-Json).bookmarks.url) -contains $TestUrl } 'Bookmark was not saved'
    if ([BrowserTest]::IsWindowVisible($edit)) { throw 'Adding a bookmark should not open the picker' }
    [void][BrowserTest]::PostMessage($window,0x8005,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-For { (Get-Content $log -Raw) -match 'bookmark_added=false' } 'Repeated bookmark action not handled'
    $saved = Get-Content $library -Raw | ConvertFrom-Json
    if (!(($saved.history | Where-Object { $_.url -eq $TestUrl }).visited_at -gt 0)) { throw 'Visit timestamp was not persisted' }
    if (@($saved.bookmarks | Where-Object { $_.url -eq $TestUrl }).Count -ne 1) { throw 'Repeated Ctrl+D removed or duplicated the bookmark' }
    # Opening the overlay must not move/resize the WebView native child.
    $web = [BrowserTest]::FindWindowEx($window, [IntPtr]::Zero, 'Chrome_WidgetWin_0', $null)
    if ($web -eq [IntPtr]::Zero) { throw 'WebView native child not found' }
    $before = New-Object BrowserTest+Rect
    $after = New-Object BrowserTest+Rect
    [void][BrowserTest]::GetWindowRect($web, [ref]$before)
    if ($CheckLeader) {
        if ($SkipFocusChecks) { throw '-CheckLeader requires foreground focus checks' }
        Add-Type -AssemblyName System.Windows.Forms, UIAutomationClient, UIAutomationTypes, System.Drawing
        [void][BrowserTest]::SetForegroundWindow($window)
        Wait-For { [BrowserTest]::GetForegroundWindow() -eq $window } 'Leader test requires foreground focus'
        $leaderPanel = [BrowserTest]::FindWindowEx($window,[IntPtr]::Zero,'WinarchyLeaderPanel',$null)
        if ($leaderPanel -eq [IntPtr]::Zero) { throw 'Leader panel missing' }
        function Send-LeaderKeys([string]$keys) {
            if ([BrowserTest]::GetForegroundWindow() -ne $window) { throw 'Focus left the test browser; refusing keyboard input' }
            [Windows.Forms.SendKeys]::SendWait($keys)
            Start-Sleep -Milliseconds 120
        }
        function Leader-Title {
            $value = New-Object Text.StringBuilder 256
            [void][BrowserTest]::GetText($leaderPanel,0x000D,[IntPtr]256,$value)
            $value.ToString()
        }
        function Assert-LeaderClosed {
            Wait-For { ![BrowserTest]::IsWindowVisible($leaderPanel) } 'Leader did not close'
        }
        $automation = [Windows.Automation.AutomationElement]::FromHandle($window)
        function Find-Accessible([string]$name) {
            $condition = New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty,$name)
            $automation.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
        }
        function Accessible-Value($element) {
            $element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value
        }
        $field = Find-Accessible 'Leader test input'
        $events = Find-Accessible 'Leader keyboard events'
        if (!$field -or !$events) { throw 'Keyboard fixture controls missing from UI Automation' }
        $field.SetFocus()
        Send-LeaderKeys 'sentinel'
        $keysBefore = Accessible-Value $events
        Send-LeaderKeys '^b'
        Wait-For { [BrowserTest]::IsWindowVisible($leaderPanel) } 'Ctrl+B did not open leader from web input'
        $leaderRect = New-Object BrowserTest+Rect
        $windowRect = New-Object BrowserTest+Rect
        [void][BrowserTest]::GetWindowRect($leaderPanel,[ref]$leaderRect)
        [void][BrowserTest]::GetWindowRect($window,[ref]$windowRect)
        $margin = [Math]::Floor(12 * [BrowserTest]::GetDpiForWindow($window) / 96)
        if ([Math]::Abs($windowRect.right-$leaderRect.right-$margin) -gt 2 -or
            [Math]::Abs($windowRect.bottom-$leaderRect.bottom-$margin) -gt 2) { throw 'Leader panel not anchored bottom-right' }
        if (($leaderRect.right-$leaderRect.left) -gt (576 * [BrowserTest]::GetDpiForWindow($window) / 96 + 1)) { throw 'Leader exceeds compact maximum width' }
        [void][BrowserTest]::GetWindowRect($web,[ref]$after)
        if ($before.left -ne $after.left -or $before.top -ne $after.top -or $before.right -ne $after.right -or $before.bottom -ne $after.bottom) { throw 'Leader resized the page' }
        $bitmap = New-Object Drawing.Bitmap ($leaderRect.right-$leaderRect.left),($leaderRect.bottom-$leaderRect.top)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($leaderRect.left,$leaderRect.top,0,0,$bitmap.Size)
            $bitmap.Save((Join-Path $root 'leader.png'))
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
        Send-LeaderKeys 'n'
        if ((Leader-Title) -notmatch 'Navigation') { throw 'Navigation submenu missing' }
        Send-LeaderKeys '{BACKSPACE}'
        if ((Leader-Title) -notmatch 'Principal') { throw 'Backspace did not return to root' }
        Send-LeaderKeys '{ESC}'
        Assert-LeaderClosed
        if ((Accessible-Value $field) -ne 'sentinel' -or (Accessible-Value $events) -ne $keysBefore) { throw 'Leader keys leaked into the page' }
        Send-LeaderKeys '^b'
        Start-Sleep -Milliseconds 4000
        if (![BrowserTest]::IsWindowVisible($leaderPanel)) { throw 'Leader expired while idle' }
        Send-LeaderKeys 'n'
        Start-Sleep -Milliseconds 4000
        if (![BrowserTest]::IsWindowVisible($leaderPanel) -or (Leader-Title) -notmatch 'Navigation') { throw 'Leader submenu expired while idle' }
        Send-LeaderKeys '{ESC}'
        Assert-LeaderClosed
        Send-LeaderKeys 'x'
        if ((Accessible-Value $field) -ne 'sentinelx') { throw 'Cancellation did not preserve web input focus' }
        $keysBefore = Accessible-Value $events
        Send-LeaderKeys '^b'
        Send-LeaderKeys '^b'
        Assert-LeaderClosed
        $newKeys = (Accessible-Value $events).Substring($keysBefore.Length)
        if ($newKeys -cne 'Ctrl+b|') { throw "Double leader did not pass exactly one Ctrl+B: $newKeys" }
        Send-LeaderKeys '^b'
        Send-LeaderKeys 'x'
        if (![BrowserTest]::IsWindowVisible($leaderPanel)) { throw 'Unknown key closed the leader' }
        if ((Accessible-Value $field) -ne 'sentinelx') { throw 'Invalid key leaked into the page' }
        Send-LeaderKeys '{ESC}'
        Assert-LeaderClosed
        Send-LeaderKeys '^b'
        Send-LeaderKeys 'f'
        Assert-LeaderClosed
        $focused = [Windows.Automation.AutomationElement]::FocusedElement
        if ($focused.Current.ControlType -ne [Windows.Automation.ControlType]::Edit -or $focused.Current.Name -eq 'Leader test input') { throw 'Find action did not focus the native find field' }
        Send-LeaderKeys 'fixture'
        if ((Accessible-Value $focused) -ne 'fixture') { throw 'Find field did not receive search query' }
        Send-LeaderKeys '{ESC}'
        # URL action and cancellation preserve a partially typed native query.
        Send-LeaderKeys '^b'
        Send-LeaderKeys 'l'
        Wait-For { [BrowserTest]::IsWindowVisible($edit) } 'Leader URL action failed'
        [void][BrowserTest]::SetText($edit,0x000C,[IntPtr]::Zero,'unfinished query')
        Send-LeaderKeys '^b'
        Send-LeaderKeys '{ESC}'
        $query = New-Object Text.StringBuilder 256
        [void][BrowserTest]::GetText($edit,0x000D,[IntPtr]256,$query)
        if ($query.ToString() -ne 'unfinished query') { throw 'Leader lost the native query' }
        Send-LeaderKeys '{ESC}'
        $frameField = Find-Accessible 'Leader frame input'
        if (!$frameField) { throw 'Frame fixture input missing' }
        $frameField.SetFocus()
        Send-LeaderKeys '^b'
        Wait-For { [BrowserTest]::IsWindowVisible($leaderPanel) } 'Leader did not open from iframe'
        Send-LeaderKeys '{ESC}'
        Assert-LeaderClosed
        Write-Host "PASS: leader menus, bounds, persistent idle state, web/native/iframe input, no key leakage, double Ctrl+B, find UI. Screenshot: $root/leader.png"
    }
    [void][BrowserTest]::PostMessage($window,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-For { [BrowserTest]::IsWindowVisible($edit) } 'Picker did not open'
    Assert-CenteredPalette
    [void][BrowserTest]::GetWindowRect($web, [ref]$after)
    if ($before.left -ne $after.left -or $before.top -ne $after.top -or
        $before.right -ne $after.right -or $before.bottom -ne $after.bottom) { throw 'Opening palette changed WebView bounds' }
    if ([BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags)) { throw 'Palette made the page translucent' }
    # Escape closes only the overlay, leaving the page in place.
    [void][BrowserTest]::PostMessage($edit,0x0100,[IntPtr]27,[IntPtr]::Zero)
    Wait-For { ![BrowserTest]::IsWindowVisible($panel) } 'Escape did not close the overlay'
    [void][BrowserTest]::PostMessage($window,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-For { [BrowserTest]::IsWindowVisible($edit) } 'Picker did not reopen'
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, 'about:blank')
    Start-Sleep -Milliseconds 200
    [void][BrowserTest]::PostMessage($edit,0x0100,[IntPtr]13,[IntPtr]::Zero)
    Wait-For { [BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) } 'Home did not return'
    Wait-For { [BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) -and $alpha -eq 115 } 'Returning home lost the override set while browsing'
    Remove-Item $opacityPath
    Wait-For { [BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) -and $alpha -eq 191 } 'Clearing override did not restore theme opacity'
    if ($CheckLeader) {
        [void][BrowserTest]::SetText($edit,0x000C,[IntPtr]::Zero,'home query')
        Send-LeaderKeys '^b'
        Wait-For { [BrowserTest]::IsWindowVisible($leaderPanel) } 'Leader did not open from home'
        Send-LeaderKeys 'n'
        Send-LeaderKeys '{ESC}'
        Assert-LeaderClosed
        $query = New-Object Text.StringBuilder 256
        [void][BrowserTest]::GetText($edit,0x000D,[IntPtr]256,$query)
        if ($query.ToString() -ne 'home query') { throw 'Leader consumed or replaced home input' }
        Write-Host 'PASS: home leader cancellation preserves the native query.'
    }
    $process.Refresh()
    if ($process.MainWindowHandle -ne $window) { throw 'Opacity update recreated the browser window' }
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, 'https')
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 2 } 'Home must search the bookmark and history together'
    Assert-CenteredPalette
    # A single result click opens it; no double click is required.
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, $TestUrl)
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 1 } 'Fixture bookmark not found'
    [void][BrowserTest]::PostMessage($list,0x0201,[IntPtr]1,[IntPtr]0x00200020)
    [void][BrowserTest]::PostMessage($list,0x0202,[IntPtr]::Zero,[IntPtr]0x00200020)
    Wait-For { ![BrowserTest]::IsWindowVisible($panel) } 'Single click did not navigate'
    Write-Host "PASS: centered home/overlay, Escape, home opacity, fuzzy suggestions, navigation, opaque page, history and bookmark persistence. Logs: $root"
} finally {
    $env:WINARCHY_CONFIG_HOME = $oldHome
    $env:LOCALAPPDATA = $oldLocal
    if ($process -and !$process.HasExited) {
        [void]$process.CloseMainWindow()
        if (!$process.WaitForExit(5000)) { Write-Warning "Test process $($process.Id) did not close; not forcibly terminating it." }
    }
}
