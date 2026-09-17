# Run explicitly, never on the browser's startup path. Restart the browser afterwards.
[CmdletBinding()]
param([string]$ConfigHome = $env:WINARCHY_CONFIG_HOME)
$ErrorActionPreference = 'Stop'
if (!$ConfigHome) { $ConfigHome = Join-Path $HOME '.config/winarchy' }
$dir = Join-Path $ConfigHome 'browser'
New-Item -ItemType Directory -Force $dir | Out-Null
# Upstream lists retain their own licenses, not Winarchy's MIT license.
# See https://easylist.to/pages/licence.html and docs/browser.md.
$lists = @{
    'easylist.txt' = 'https://easylist.to/easylist/easylist.txt'
    'easyprivacy.txt' = 'https://easylist.to/easylist/easyprivacy.txt'
}
foreach ($name in $lists.Keys) {
    $temp = Join-Path $dir ($name + '.' + [guid]::NewGuid().ToString() + '.tmp')
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $lists[$name] -OutFile $temp -TimeoutSec 120
        $file = Get-Item $temp
        if ($file.Length -lt 1024 -or $file.Length -gt 20MB) { throw "Unexpected filter list size: $name" }
        $first = Get-Content $temp -TotalCount 1
        if ($first -notmatch '^\[Adblock') { throw "Invalid filter list header: $name" }
        Move-Item -Force $temp (Join-Path $dir $name)
        Write-Host "Updated $name from $($lists[$name])"
    } finally {
        if (Test-Path $temp) { Remove-Item $temp }
    }
}
# Old compiled caches are rebuildable, never user data.
Get-ChildItem $dir -Filter 'filters-adblock-*.bin' | Remove-Item
Write-Host 'Restart winarchy-browser to use these lists.'
