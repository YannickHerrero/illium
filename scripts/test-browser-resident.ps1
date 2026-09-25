# Windows-only. Refuses to use an existing resident. Uses disposable config/profile.
# Close/stop any browser resident first. No user profile or bookmarks are touched.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [string]$Filters = "$env:USERPROFILE/.config/illium/browser"
)
$ErrorActionPreference = 'Stop'
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $until = [DateTime]::UtcNow.AddSeconds(30)
    while (!( & $Condition )) {
        if ([DateTime]::UtcNow -gt $until) { throw $Message }
        Start-Sleep -Milliseconds 100
    }
}
function Control([string]$Arguments) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $Exe
    $info.Arguments = $Arguments
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $client = [Diagnostics.Process]::Start($info)
    if (!$client.WaitForExit(12000)) { throw 'Control command timed out; do not retry open blindly' }
    $out = $client.StandardOutput.ReadToEnd()
    $err = $client.StandardError.ReadToEnd()
    if ($client.ExitCode -ne 0) { throw "Control failed: $err" }
    return $out
}
function Warm-Open {
    # Like the daemon fast path: no new browser client process in the timed path.
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $session=[Diagnostics.Process]::GetCurrentProcess().SessionId
    $pipe=New-Object IO.Pipes.NamedPipeClientStream '.',("illium-browser-$sid-$session"),([IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect(3000)
        $writer=New-Object IO.StreamWriter $pipe,(New-Object Text.UTF8Encoding $false)
        $writer.AutoFlush=$true
        $reader=New-Object IO.StreamReader $pipe
        $writer.WriteLine('open ""')
        $line=$reader.ReadLineAsync()
        if (!$line.Wait(5000)) { throw 'Warm open timed out; outcome unknown, do not retry' }
        $reply=$line.Result | ConvertFrom-Json
        $pipe.WriteByte(6)
        if (!$reply.ok) { throw $reply.message }
    } finally { $pipe.Dispose() }
}
function Memory([int]$RootId) {
    $all = @(Get-CimInstance Win32_Process)
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootId)
    do {
        $added=$false
        foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId) -and $ids.Add([int]$p.ProcessId)) { $added=$true } }
    } while ($added)
    $private=0L; $working=0L; $count=0
    foreach ($id in $ids) {
        $p=Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($p) { $count++; $private+=$p.PrivateMemorySize64; $working+=$p.WorkingSet64 }
    }
    return @{ processes=$count; private_commit_mib=[Math]::Round($private/1MB,1); working_set_sum_mib=[Math]::Round($working/1MB,1) }
}
$root = Join-Path $env:TEMP ('illium-browser-resident-test-' + [guid]::NewGuid())
$config = Join-Path $root 'config'
$filterDir = Join-Path $config 'browser'
New-Item -ItemType Directory -Force $filterDir, (Join-Path $config 'themes') | Out-Null
[IO.File]::WriteAllText((Join-Path $config 'illium.toml'), 'theme = "test"')
$theme = (Get-Content "$PSScriptRoot/../config/themes/catppuccin-mocha.toml" -Raw) + "`nbackground_opacity = 0.75`n"
[IO.File]::WriteAllText((Join-Path $config 'themes/test.toml'), $theme)
Get-ChildItem $Filters -File | Where-Object { $_.Extension -in '.txt','.bin' } | Copy-Item -Destination $filterDir
if (!(Get-ChildItem $filterDir -Filter '*.txt')) { throw 'Download filter lists first.' }
$oldHome=$env:ILLIUM_CONFIG_HOME; $oldLocal=$env:LOCALAPPDATA
$resident=$null; $cold=$null; $extra=$null; $owned=$false
try {
    $env:ILLIUM_CONFIG_HOME=$config
    $env:LOCALAPPDATA=Join-Path $root 'local'
    $coldLog=Join-Path $root 'cold.log'
    $cold=Start-Process $Exe -ArgumentList '--standalone' -RedirectStandardError $coldLog -PassThru
    Wait-For { (Get-Content $coldLog -Raw -ErrorAction SilentlyContinue) -match 'webview_ready_ms=' } 'Cold browser not ready'
    $coldMemory=Memory $cold.Id
    [void]$cold.CloseMainWindow()
    if (!$cold.WaitForExit(10000)) { throw 'Cold browser did not close' }
    $log=Join-Path $root 'resident.log'
    $resident=Start-Process $Exe -ArgumentList '--serve' -RedirectStandardError $log -PassThru
    Wait-For {
        if ($resident.HasExited) { throw 'No test resident elected (another resident may be running). Stop it explicitly before this test.' }
        (Get-Content $log -Raw -ErrorAction SilentlyContinue) -match 'resident_ready'
    } 'Resident did not become ready'
    $owned=$true
    $state=Control '--status' | ConvertFrom-Json
    if ($state.pid -ne $resident.Id -or $state.opened) { throw 'Expected our hidden ready resident' }
    $resident.Refresh()
    if ($resident.MainWindowHandle -ne [IntPtr]::Zero) { throw 'Prewarm unexpectedly showed a window' }
    # Palette wakeups and resident IPC must have distinct Win32 message IDs.
    # Exercise both while hidden; a collision silently swallows status/open/quit.
    [IO.File]::WriteAllText((Join-Path $config 'background-opacity.state'), "theme = 'test'`nopacity = 0.60`n")
    Start-Sleep -Milliseconds 250
    $state=Control '--status' | ConvertFrom-Json
    if ($state.pid -ne $resident.Id -or $state.opened) { throw 'Opacity update broke hidden resident IPC' }
    $idleMemory=Memory $resident.Id
    $clock=[Diagnostics.Stopwatch]::StartNew()
    Warm-Open
    $clock.Stop()
    $state=Control '--status' | ConvertFrom-Json
    if (!$state.opened -or $state.pid -ne $resident.Id -or $null -eq $state.warm_open_ms) { throw 'Prepared window was not used' }
    $firstShow=$state.warm_open_ms
    $firstRoundtrip=$clock.Elapsed.TotalMilliseconds
    # While occupied, another open must create a different window, not hijack it.
    [void](Control '')
    Wait-For {
        $script:extra = Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $resident.Id -and $_.Name -eq [IO.Path]::GetFileName($Exe) } | Select-Object -First 1
        $null -ne $script:extra
    } 'Second window did not launch separately'
    $extra=Get-Process -Id $extra.ProcessId
    Wait-For { $extra.Refresh(); $extra.MainWindowHandle -ne [IntPtr]::Zero } 'Second window not visible'
    [void]$extra.CloseMainWindow()
    if (!$extra.WaitForExit(10000)) { throw 'Second window did not close' }
    $resident.Refresh()
    [void]$resident.CloseMainWindow()
    Wait-For { $state=Control '--status' | ConvertFrom-Json; !$state.opened } 'Fresh hidden window was not rebuilt'
    $rebuiltMemory=Memory $resident.Id
    [void](Control '')
    $state=Control '--status' | ConvertFrom-Json
    if (!$state.opened -or $null -eq $state.warm_open_ms) { throw 'Rebuilt window was not warm' }
    $secondShow=$state.warm_open_ms
    # Quitting the daemon/resident must preserve an open page, then stop on close.
    [void](Control '--quit')
    if ($resident.HasExited) { throw 'Quit must not destroy an open user window' }
    $resident.Refresh(); [void]$resident.CloseMainWindow()
    if (!$resident.WaitForExit(10000)) { throw 'Resident did not stop after the window closed' }
    $idleLog=Join-Path $root 'idle-quit.log'
    $resident=Start-Process $Exe -ArgumentList '--serve' -RedirectStandardError $idleLog -PassThru
    Wait-For { (Get-Content $idleLog -Raw -ErrorAction SilentlyContinue) -match 'resident_ready' } 'Idle shutdown fixture not ready'
    [void](Control '--quit')
    if (!$resident.WaitForExit(10000)) { throw 'Idle resident did not stop' }
    $report=@{
        cold_startup_log=[string](Get-Content $coldLog -Raw)
        cold_home_memory=$coldMemory
        warm_idle_memory=$idleMemory
        rebuilt_idle_memory=$rebuiltMemory
        first_warm_show_ms=$firstShow
        first_warm_ipc_roundtrip_ms=[Math]::Round($firstRoundtrip,1)
        second_warm_show_ms=$secondShow
        note='One run, not a benchmark. ShowWindow timing is not first paint. Working-set sums double-count shared pages.'
    }
    $report | ConvertTo-Json -Depth 4 | Tee-Object -FilePath (Join-Path $root 'results.json')
    Write-Host "PASS: prepared opening, independent extra window, rebuilt spare, deferred and idle shutdown. Results: $root"
} finally {
    if ($owned -and $resident -and !$resident.HasExited) {
        try { [void](Control '--quit') } catch { Write-Warning $_ }
        $resident.Refresh(); [void]$resident.CloseMainWindow()
    }
    if ($cold -and !$cold.HasExited) { [void]$cold.CloseMainWindow() }
    if ($extra -is [Diagnostics.Process] -and !$extra.HasExited) { [void]$extra.CloseMainWindow() }
    $env:ILLIUM_CONFIG_HOME=$oldHome; $env:LOCALAPPDATA=$oldLocal
}
