# Requires the updated running daemon and its matching configuration environment.
# Does not switch workspaces, close windows or restart applets. Briefly decreases
# opacity and restores only its own override. Tests coalesced create/delete events.
param([Parameter(Mandatory = $true)][string]$Bin)
$ErrorActionPreference = 'Stop'
$diagnostics = Join-Path $env:TEMP ('illium-bar-opacity-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $diagnostics | Out-Null
function Ctl([string]$command) {
    $out = Join-Path $diagnostics 'verify-out.txt'
    $err = Join-Path $diagnostics 'verify-err.txt'
    $p = Start-Process (Join-Path $bin 'illiumctl.exe') -ArgumentList $command -WorkingDirectory $bin -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err -PassThru
    $handle = $p.Handle
    if (-not $p.WaitForExit(10000)) { $p.Kill(); throw 'CLI timed out' }
    if ($p.ExitCode -ne 0) { throw [IO.File]::ReadAllText($err) }
    return [IO.File]::ReadAllText($out)
}
$before = (Ctl 'status') | ConvertFrom-Json
if ($null -eq $before.bar_background_opacity) { throw 'No bar opacity status' }
$config = if ($env:ILLIUM_CONFIG_HOME) { $env:ILLIUM_CONFIG_HOME } else { Join-Path $env:USERPROFILE '.config\illium' }
$state = Join-Path $config 'background-opacity.state'
$previous = if (Test-Path $state) { [IO.File]::ReadAllBytes($state) } else { $null }
$ownState = $null
try {
    [void](Ctl 'opacity decrease')
    $ownState = [IO.File]::ReadAllBytes($state)
    $after = (Ctl 'status') | ConvertFrom-Json
    $expected = [Math]::Max(0.05, $before.bar_background_opacity - 0.05)
    if ([Math]::Abs($after.bar_background_opacity - $expected) -gt 0.001) { throw 'Bar did not follow the opacity command' }
} finally {
    # Do not overwrite a concurrent user change.
    if ($ownState -and (Test-Path $state) -and [Convert]::ToBase64String([IO.File]::ReadAllBytes($state)) -eq [Convert]::ToBase64String($ownState)) {
        if ($null -eq $previous) { Remove-Item $state }
        else { [IO.File]::WriteAllBytes($state, $previous) }
    }
}
$deadline = [DateTime]::UtcNow.AddSeconds(5)
do {
    Start-Sleep -Milliseconds 200
    $restored = (Ctl 'status') | ConvertFrom-Json
    if ([Math]::Abs($restored.bar_background_opacity - $before.bar_background_opacity) -lt 0.001) { break }
} while ([DateTime]::UtcNow -lt $deadline)
if ([Math]::Abs($restored.bar_background_opacity - $before.bar_background_opacity) -gt 0.001) { throw 'Bar did not follow external override reset' }
[pscustomobject]@{ DemoAvailable = ($null -ne $restored.demo_pending); OpacityBefore = $before.bar_background_opacity; OpacityDecreased = $after.bar_background_opacity; OpacityRestored = $restored.bar_background_opacity; Theme = $restored.theme; WorkspaceUnchanged = ($restored.workspace -eq $before.workspace); ClientsUnchanged = ((@($restored.clients.id) -join ',') -eq (@($before.clients.id) -join ',')) } | ConvertTo-Json
Remove-Item -Recurse -Force $diagnostics
