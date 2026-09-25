param([string]$Action = "", [switch]$FunctionsOnly)
# Slow discovery/actions only. Native Illium sampling supplies live traffic.
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$script:error_text = ""

function Read-Action([string]$text) {
  if (-not $text) { return ,@("refresh", "", "") }
  if ($text.StartsWith("[")) {
    $parts = ConvertFrom-Json -InputObject $text
    if ($parts.Count -ne 3 -or @($parts | Where-Object { $_ -isnot [string] }).Count) { throw "Invalid Wi-Fi action" }
    return ,$parts
  }
  # Compatibility with the previous one-argument view; the new view uses JSON.
  $parts = $text.Split("|")
  if ($parts.Count -gt 3) { throw "Use the updated Wi-Fi view for names or keys containing |" }
  return ,@($parts[0], $(if ($parts.Count -ge 2) { $parts[1] } else { "" }), $(if ($parts.Count -ge 3) { $parts[2] } else { "" }))
}
function Invoke-Netsh([string[]]$arguments) {
  $lines = @(& netsh @arguments 2>&1 | ForEach-Object { "$_" })
  if ($LASTEXITCODE -ne 0) {
    $message = ($lines | Where-Object { $_.Trim() } | Select-Object -First 3) -join " "
    if (-not $script:error_text) { $script:error_text = $message.Trim() }
  }
  return $lines
}
function Parse-Interfaces([string[]]$lines) {
  $result = @(); $current = $null
  foreach ($line in $lines) {
    if ($line -match '^\s+(Name|Nom)\s*:\s*(.+)$') {
      if ($null -ne $current) { $result += $current }
      $current = @{ name = $Matches[2].Trim(); guid = ""; ssid = ""; signal = 0; rate = 0.0; band = "—" }
    }
    elseif ($null -ne $current) {
      if ($line -match '^\s+GUID\s*:\s*(.+)$') { $current.guid = $Matches[1].Trim() }
      elseif ($line -match '^\s+SSID\s*:\s*(.+)$') { $current.ssid = $Matches[1].Trim() }
      elseif ($line -match '^\s+Signal\s*:\s*(\d+)') { $current.signal = [Math]::Min(100, [int]$Matches[1]) }
      elseif ($line -match '^\s+(Receive rate|R.ception)[^:]*:\s*([\d.,]+)') { $current.rate = [double]::Parse(($Matches[2] -replace ',', '.'), [Globalization.CultureInfo]::InvariantCulture) }
      elseif ($line -match '^\s+(Band|Bande)\s*:\s*(.+)$') { $current.band = $Matches[2].Trim() }
    }
  }
  if ($null -ne $current) { $result += $current }
  return $result
}
function Parse-Profiles([string[]]$lines) {
  @($lines | Where-Object { $_ -match '^\s*(All User Profile|Current User Profile|Profil[^:]*)\s*:\s*(.+)$' } | ForEach-Object { $Matches[2].Trim() } | Where-Object { $_ } | Select-Object -Unique -First 128)
}
function Parse-Networks([string[]]$lines, [string[]]$known, [string]$ssid) {
  # Ordinal keys: SSIDs are case-sensitive, unlike PowerShell's default hashtable.
  $networks = New-Object 'System.Collections.Generic.Dictionary[string,object]' ([StringComparer]::Ordinal)
  $current = $null
  foreach ($line in $lines) {
    if ($line -match '^\s*SSID\s+\d+\s*:\s*(.*)$') {
      $name = $Matches[1].Trim()
      if (-not $name) { $current = $null; continue }
      if (-not $networks.ContainsKey($name)) {
        $networks[$name] = [ordered]@{ ssid = $name; signal = 0; secured = $true; known = (($known -ccontains $name) -or ($name -ceq $ssid)); connected = ($name -ceq $ssid); available = $true }
      }
      $current = $networks[$name]
    }
    elseif ($null -ne $current -and $line -match '^\s+Auth[^:]*:\s*(.+)$') { $current.secured = -not ($Matches[1] -match '^(Open|Ouvert)') }
    elseif ($null -ne $current -and $line -match '^\s+Signal\s*:\s*(\d+)') { $current.signal = [Math]::Min(100, [Math]::Max($current.signal, [int]$Matches[1])) }
  }
  if ($ssid -and -not $networks.ContainsKey($ssid)) {
    $networks[$ssid] = [ordered]@{ ssid = $ssid; signal = 0; secured = $true; known = $true; connected = $true; available = $true }
  }
  foreach ($name in $known) {
    if (-not $networks.ContainsKey($name)) { $networks[$name] = [ordered]@{ ssid = $name; signal = 0; secured = $true; known = $true; connected = ($name -ceq $ssid); available = ($name -ceq $ssid) } }
  }
  @($networks.Values | Sort-Object -Property @{ Expression = { $_.connected }; Descending = $true }, @{ Expression = { $_.known }; Descending = $true }, @{ Expression = { $_.signal }; Descending = $true }, @{ Expression = { $_.ssid } } | Select-Object -First 64)
}
function New-ProfileXml([string]$ssid, [string]$key) {
  $ssidBytes = [Text.Encoding]::UTF8.GetBytes($ssid)
  if ($ssidBytes.Length -lt 1 -or $ssidBytes.Length -gt 32) { throw "The SSID must contain 1 to 32 UTF-8 bytes." }
  if ($key -and ($key.Length -lt 8 -or $key.Length -gt 63)) { throw "The WPA2 password must contain 8 to 63 characters." }
  $name = [Security.SecurityElement]::Escape($ssid)
  $hex = ($ssidBytes | ForEach-Object { "{0:X2}" -f $_ }) -join ""
  $security = if ($key) {
    "<authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption>" +
    "<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>$([Security.SecurityElement]::Escape($key))</keyMaterial></sharedKey>"
  } else { "<authEncryption><authentication>open</authentication><encryption>none</encryption><useOneX>false</useOneX></authEncryption>" }
  "<?xml version=`"1.0`"?><WLANProfile xmlns=`"http://www.microsoft.com/networking/WLAN/profile/v1`"><name>$name</name><SSIDConfig><SSID><hex>$hex</hex><name>$name</name></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>auto</connectionMode><MSM><security>$security</security></MSM></WLANProfile>"
}
function Add-Profile([string]$ssid, [string]$key, [string]$interface) {
  $xml = New-ProfileXml $ssid $key
  # A private directory avoids a predictable shared file containing a cleartext key.
  $directory = Join-Path $env:TEMP ("illium-wifi-" + [Guid]::NewGuid().ToString("N"))
  $acl = New-Object Security.AccessControl.DirectorySecurity
  $acl.SetAccessRuleProtection($true, $false)
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
  $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, "FullControl", "ContainerInherit,ObjectInherit", "None", "Allow")
  $acl.AddAccessRule($rule)
  [void][IO.Directory]::CreateDirectory($directory, $acl)
  try {
    $file = Join-Path $directory "profile.xml"
    [IO.File]::WriteAllText($file, $xml, (New-Object Text.UTF8Encoding($false)))
    $null = Invoke-Netsh @("wlan", "add", "profile", "filename=$file", "interface=$interface", "user=current")
  } finally { Remove-Item -LiteralPath $directory -Recurse -Force }
}

if ($FunctionsOnly) { return }
$data = [ordered]@{ connected = $false; ssid = ""; signal = 0; rate = 0.0; band = "—"; interface_guid = ""; ip = "—"; gateway = "—"; dns = "—"; link_rate = "—"; status = "Wi-Fi disconnected"; networks = @(); error = "" }
try {
  $parts = Read-Action $Action
  $interfaces = @(Parse-Interfaces (Invoke-Netsh @("wlan", "show", "interfaces")))
  $selected = @($interfaces | Where-Object { $_.ssid } | Select-Object -First 1)
  if (-not $selected.Count) { $selected = @($interfaces | Select-Object -First 1) }
  $interface = if ($selected.Count) { $selected[0].name } else { "" }
  $known = if ($interface) { @(Parse-Profiles (Invoke-Netsh @("wlan", "show", "profiles", "interface=$interface"))) } else { @() }
  switch ($parts[0]) {
    "refresh" { }
    "settings" { Start-Process "ms-settings:network-wifi" }
    "connect" {
      if (-not $interface -or -not $parts[1]) { throw "No network selected." }
      if ($parts[2] -or $known -cnotcontains $parts[1]) { Add-Profile $parts[1] $parts[2] $interface }
      if (-not $script:error_text) {
        $null = Invoke-Netsh @("wlan", "connect", "name=$($parts[1])", "ssid=$($parts[1])", "interface=$interface")
        # Poll an actual connection result instead of reporting command acknowledgement as success.
        for ($i = 0; $i -lt 6 -and -not $script:error_text; $i++) {
          Start-Sleep -Milliseconds 700
          $selected = @(Parse-Interfaces (Invoke-Netsh @("wlan", "show", "interfaces")) | Where-Object { $_.name -ceq $interface })
          if ($selected.Count -and $selected[0].ssid -ceq $parts[1]) { break }
        }
        if (-not $script:error_text -and (-not $selected.Count -or $selected[0].ssid -cne $parts[1])) { $script:error_text = "Connection not confirmed. Check the password or open Windows Settings (WPA3/enterprise)." }
      }
    }
    "disconnect" {
      if ($interface) {
        $null = Invoke-Netsh @("wlan", "disconnect", "interface=$interface")
        for ($i = 0; $i -lt 6 -and -not $script:error_text; $i++) {
          Start-Sleep -Milliseconds 300
          $selected = @(Parse-Interfaces (Invoke-Netsh @("wlan", "show", "interfaces")) | Where-Object { $_.name -ceq $interface })
          if (-not $selected.Count -or -not $selected[0].ssid) { break }
        }
      }
    }
    "forget" {
      if ($parts[1].IndexOfAny([char[]]'*?') -ge 0) { throw "Use Windows Settings to forget a profile containing wildcard characters." }
      if ($interface -and $parts[1]) { $null = Invoke-Netsh @("wlan", "delete", "profile", "name=$($parts[1])", "interface=$interface") }
    }
    default { throw "Unknown Wi-Fi action." }
  }
  if ($parts[0] -in @("connect", "disconnect", "forget")) {
    $selected = @(Parse-Interfaces (Invoke-Netsh @("wlan", "show", "interfaces")) | Where-Object { $_.name -ceq $interface })
    $known = @(Parse-Profiles (Invoke-Netsh @("wlan", "show", "profiles", "interface=$interface")))
  }
  if ($selected.Count) {
    $wifi = $selected[0]
    $data.connected = [bool]$wifi.ssid; $data.ssid = $wifi.ssid; $data.signal = $wifi.signal; $data.rate = $wifi.rate; $data.band = $wifi.band; $data.interface_guid = $wifi.guid
    $data.status = if ($data.connected) { "Connected to Wi-Fi · Signal $($wifi.signal)%" } else { "Wi-Fi disconnected" }
    if ($wifi.rate -gt 0) { $data.link_rate = "$([Math]::Round($wifi.rate)) Mbps" }
    $data.networks = @(Parse-Networks (Invoke-Netsh @("wlan", "show", "networks", "mode=bssid", "interface=$interface")) $known $wifi.ssid)
    if ($data.connected) {
      $adapter = [Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object { $_.Id.Trim('{}') -ieq $wifi.guid.Trim('{}') } | Select-Object -First 1
      if ($null -ne $adapter) {
        $properties = $adapter.GetIPProperties()
        $ip = @($properties.UnicastAddresses | Where-Object { $_.Address.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork } | ForEach-Object { $_.Address.ToString() })
        if (-not $ip.Count) { $ip = @($properties.UnicastAddresses | ForEach-Object { $_.Address.ToString() }) }
        $gateway = @($properties.GatewayAddresses | ForEach-Object { $_.Address.ToString() })
        $dns = @($properties.DnsAddresses | ForEach-Object { $_.ToString() })
        if ($ip.Count) { $data.ip = $ip[0] }
        if ($gateway.Count) { $data.gateway = $gateway[0] }
        if ($dns.Count) { $data.dns = $dns -join ", " }
      }
    }
  } elseif (-not $script:error_text) { $data.status = "No Wi-Fi adapter available" }
} catch { if (-not $script:error_text) { $script:error_text = $_.Exception.Message } }
$data.error = $script:error_text
$data | ConvertTo-Json -Compress -Depth 4
