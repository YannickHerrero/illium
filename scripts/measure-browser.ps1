# Samples the host AND its WebView2 descendants. Run with no other browser instance.
[CmdletBinding()]
param(
    [string]$Exe = "$PSScriptRoot/../target/release/winarchy-browser.exe",
    [string]$Url = 'about:blank',
    [int]$Seconds = 20,
    [string]$Output = 'browser-measurement.csv'
)
$ErrorActionPreference = 'Stop'
if (!$PSBoundParameters.ContainsKey('Exe') -and !(Test-Path $Exe) -and (Test-Path "$PSScriptRoot/../winarchy-browser.exe")) {
    $Exe = "$PSScriptRoot/../winarchy-browser.exe"
}
if (Get-Process winarchy-browser -ErrorAction SilentlyContinue) { throw 'Close other winarchy-browser instances first.' }
if ($Url.Contains('"')) { throw 'URL must not contain literal quotes.' }
$log = [IO.Path]::GetFullPath("$Output.startup.log")
$process = Start-Process -FilePath $Exe -ArgumentList ('--standalone "' + $Url + '"') -RedirectStandardError $log -PassThru
$known = [Collections.Generic.HashSet[int]]::new()
[void]$known.Add($process.Id)
$rows = @()
$clock = [Diagnostics.Stopwatch]::StartNew()
while ($clock.Elapsed.TotalSeconds -lt $Seconds) {
    $all = @(Get-CimInstance Win32_Process)
    # Parent relationships may be deeper than one level or disappear after exit.
    do {
        $added = $false
        foreach ($p in $all) {
            if ($known.Contains([int]$p.ParentProcessId) -and $known.Add([int]$p.ProcessId)) { $added = $true }
        }
    } while ($added)
    $live = @($all | Where-Object { $known.Contains([int]$_.ProcessId) })
    $private = 0L; $working = 0L; $cpu = 0.0
    foreach ($p in $live) {
        $stats = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
        if ($stats) { $private += $stats.PrivateMemorySize64; $working += $stats.WorkingSet64; $cpu += $stats.TotalProcessorTime.TotalSeconds }
    }
    $rows += [pscustomobject]@{
        elapsed_s = [math]::Round($clock.Elapsed.TotalSeconds, 3)
        processes = $live.Count
        private_commit_bytes = $private
        working_set_sum_bytes = $working
        cpu_seconds_live_processes = $cpu
    }
    Start-Sleep -Milliseconds 500
}
$rows | Export-Csv -NoTypeInformation $Output
Write-Host "Saved $Output and $log. Working-set sums double-count shared pages."
Write-Host 'Browser left open intentionally: close it normally, then verify its WebView2 processes exit.'
