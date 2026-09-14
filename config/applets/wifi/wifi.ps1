# Prints the wireless status as JSON for the Winarchy wifi applet.
# netsh output is localized; values are matched on label fragments common to
# English and French Windows.
$ErrorActionPreference = "SilentlyContinue"
$lines = netsh wlan show interfaces 2>$null
$ssid = ""; $signal = 0; $rate = 0.0; $band = ""
foreach ($line in $lines) {
  if ($line -match '^\s+SSID\s*:\s*(.+)$') { $ssid = $Matches[1].Trim() }
  elseif ($line -match '^\s+Signal\s*:\s*(\d+)') { $signal = [int]$Matches[1] }
  elseif ($line -match '^\s+(Receive rate|R.ception)[^:]*:\s*([\d.,]+)') { $rate = [double]($Matches[2] -replace ',', '.') }
  elseif ($line -match '^\s+(Band|Bande)\s*:\s*(.+)$') { $band = $Matches[2].Trim() }
}
[ordered]@{
  connected = [bool]$ssid
  ssid = $(if ($ssid) { $ssid } else { "" })
  signal = $signal
  rate = $rate
  band = $band
} | ConvertTo-Json -Compress
