# Explicit Windows desktop smoke test. Opens and closes ONLY its own browser process.
# Start the fixture first: python -m http.server 8765 --bind 127.0.0.1 --directory tests/browser
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [string]$TestUrl = 'http://127.0.0.1:8765/index.html'
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BrowserTest {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr SetText(IntPtr h, uint m, IntPtr w, string s);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr h, out uint key, out byte alpha, out uint flags);
}
'@
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
    $process = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $log
    Wait-For { $process.Refresh(); $process.MainWindowHandle -ne [IntPtr]::Zero } 'No browser window'
    Wait-For { (Get-Content $log -Raw -ErrorAction SilentlyContinue) -match 'webview_ready_ms=' } 'WebView not ready'
    $window = $process.MainWindowHandle
    $edit = [BrowserTest]::GetDlgItem($window, 101)
    $list = [BrowserTest]::GetDlgItem($window, 102)
    if (![BrowserTest]::IsWindowVisible($edit)) { throw 'Home input is not visible' }
    [uint32]$key=0; [byte]$alpha=0; [uint32]$flags=0
    if (![BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags) -or $alpha -ne 217) { throw 'Home alpha is not 85%' }
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, 'bkmd')
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 1 } 'Fuzzy bookmark match missing'
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, 'zzzzzzzz')
    Wait-For { [BrowserTest]::SendMessage($list,0x018B,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -eq 0 } 'Unrelated suggestion was retained'
    [void][BrowserTest]::SetText($edit, 0x000C, [IntPtr]::Zero, $TestUrl)
    Start-Sleep -Milliseconds 200
    [void][BrowserTest]::PostMessage($edit,0x0100,[IntPtr]13,[IntPtr]::Zero)
    $library = Join-Path $filters 'library.json'
    Wait-For { ((Get-Content $library -Raw | ConvertFrom-Json).history.url) -contains $TestUrl } 'Successful visit was not recorded'
    if ([BrowserTest]::IsWindowVisible($edit)) { throw 'Home input still visible on the page' }
    if ([BrowserTest]::GetLayeredWindowAttributes($window,[ref]$key,[ref]$alpha,[ref]$flags)) { throw 'Page should no longer be layered/translucent' }
    # Invoke the same queued action as Ctrl+D without injecting keys into the user's desktop.
    [void][BrowserTest]::PostMessage($window,0x8005,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-For { ((Get-Content $library -Raw | ConvertFrom-Json).bookmarks.url) -contains $TestUrl } 'Bookmark was not saved'
    [void][BrowserTest]::PostMessage($window,0x8005,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-For { ((Get-Content $library -Raw | ConvertFrom-Json).bookmarks.url) -notcontains $TestUrl } 'Bookmark toggle did not remove the entry'
    Write-Host "PASS: home opacity, fuzzy suggestions, navigation, opaque page, history and bookmark persistence. Logs: $root"
} finally {
    $env:WINARCHY_CONFIG_HOME = $oldHome
    $env:LOCALAPPDATA = $oldLocal
    if ($process -and !$process.HasExited) {
        [void]$process.CloseMainWindow()
        if (!$process.WaitForExit(5000)) { Write-Warning "Test process $($process.Id) did not close; not forcibly terminating it." }
    }
}
