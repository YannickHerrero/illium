# Wireless status and nearby networks as JSON for the Winarchy wifi applet.
# The first argument is the action requested by the view:
#   (none) | refresh          status and network list
#   connect|<ssid>            connect to a known or open network
#   connect|<ssid>|<key>      add a WPA2 profile with this key, then connect
#   disconnect                drop the current connection
#   forget|<ssid>             delete the saved profile
# netsh output is localized; values are matched on label fragments common to
# English and French Windows.
$ErrorActionPreference = "SilentlyContinue"
$action = if ($args.Count -gt 0) { [string]$args[0] } else { "" }
$error_text = ""

function Escape-Xml([string]$s) {
  $s.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;").Replace('"', "&quot;")
}
function Run-Netsh([string[]]$netshArgs) {
  $out = & netsh @netshArgs 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { $script:error_text = ($out -split "`n" | Where-Object { $_.Trim() } | Select-Object -First 1).Trim() }
}
function Add-Profile([string]$ssid, [string]$key) {
  $name = Escape-Xml $ssid
  $hex = ($ssid.ToCharArray() | ForEach-Object { "{0:X2}" -f [int]$_ }) -join ""
  $security = if ($key) {
    "<authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption>" +
    "<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>$(Escape-Xml $key)</keyMaterial></sharedKey>"
  } else {
    "<authEncryption><authentication>open</authentication><encryption>none</encryption><useOneX>false</useOneX></authEncryption>"
  }
  $xml = "<?xml version=`"1.0`"?><WLANProfile xmlns=`"http://www.microsoft.com/networking/WLAN/profile/v1`">" +
    "<name>$name</name><SSIDConfig><SSID><hex>$hex</hex><name>$name</name></SSID></SSIDConfig>" +
    "<connectionType>ESS</connectionType><connectionMode>auto</connectionMode>" +
    "<MSM><security>$security</security></MSM></WLANProfile>"
  $file = Join-Path $env:TEMP "winarchy-wifi-profile.xml"
  Set-Content -Path $file -Value $xml -Encoding UTF8
  Run-Netsh @("wlan", "add", "profile", "filename=`"$file`"", "user=current")
  Remove-Item $file -Force
}

$parts = $action.Split("|")
switch ($parts[0]) {
  "connect" {
    $ssid = if ($parts.Count -ge 3) { $parts[1..($parts.Count - 2)] -join "|" } elseif ($parts.Count -eq 2) { $parts[1] } else { "" }
    $key = if ($parts.Count -ge 3) { $parts[-1] } else { $null }
    if ($ssid) {
      $profiles = netsh wlan show profiles 2>$null | Where-Object { $_ -match 'Profil[^:]*:\s*(.+)$' } | ForEach-Object { $Matches[1].Trim() }
      if ($null -ne $key -or $profiles -notcontains $ssid) { Add-Profile $ssid $key }
      if (-not $error_text) {
        Run-Netsh @("wlan", "connect", "name=`"$ssid`"")
        Start-Sleep -Seconds 4
      }
    }
  }
  "disconnect" { Run-Netsh @("wlan", "disconnect"); Start-Sleep -Seconds 1 }
  "forget" {
    $ssid = $parts[1..($parts.Count - 1)] -join "|"
    if ($ssid) { Run-Netsh @("wlan", "delete", "profile", "name=`"$ssid`"") }
  }
}

$lines = netsh wlan show interfaces 2>$null
$ssid = ""; $signal = 0; $rate = 0.0; $band = ""
foreach ($line in $lines) {
  if ($line -match '^\s+SSID\s*:\s*(.+)$') { $ssid = $Matches[1].Trim() }
  elseif ($line -match '^\s+Signal\s*:\s*(\d+)') { $signal = [int]$Matches[1] }
  elseif ($line -match '^\s+(Receive rate|R.ception)[^:]*:\s*([\d.,]+)') { $rate = [double]($Matches[2] -replace ',', '.') }
  elseif ($line -match '^\s+(Band|Bande)\s*:\s*(.+)$') { $band = $Matches[2].Trim() }
}
$known = @(netsh wlan show profiles 2>$null | Where-Object { $_ -match 'Profil[^:]*:\s*(.+)$' } | ForEach-Object { $Matches[1].Trim() })
$networks = @{}
$order = New-Object System.Collections.ArrayList
$current = $null
foreach ($line in (netsh wlan show networks mode=bssid 2>$null)) {
  if ($line -match '^SSID\s+\d+\s*:\s*(.*)$') {
    $name = $Matches[1].Trim()
    if (-not $name) { $current = $null; continue }
    if (-not $networks.ContainsKey($name)) {
      $networks[$name] = [ordered]@{ ssid = $name; signal = 0; secured = $true; known = ($known -contains $name); connected = ($name -eq $ssid) }
      [void]$order.Add($name)
    }
    $current = $networks[$name]
  }
  elseif ($null -ne $current -and $line -match '^\s+Auth[^:]*:\s*(.+)$') { $current.secured = -not ($Matches[1] -match '^(Open|Ouvert)') }
  elseif ($null -ne $current -and $line -match '^\s+Signal\s*:\s*(\d+)') { $current.signal = [Math]::Max($current.signal, [int]$Matches[1]) }
}
$list = @($order | ForEach-Object { $networks[$_] } | Sort-Object -Property @{ Expression = "connected"; Descending = $true }, @{ Expression = "signal"; Descending = $true })
[ordered]@{
  connected = [bool]$ssid
  ssid = $(if ($ssid) { $ssid } else { "" })
  signal = $signal
  rate = $rate
  band = $band
  networks = $list
  error = $error_text
} | ConvertTo-Json -Compress -Depth 3
