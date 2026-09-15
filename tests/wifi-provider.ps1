param([string]$Provider = (Join-Path $PSScriptRoot "../config/applets/wifi/wifi.ps1"))
$ErrorActionPreference = "Stop"
. $Provider -FunctionsOnly
function Assert($condition, [string]$message) { if (-not $condition) { throw $message } }
$en = @'
    Name : Wi-Fi
    GUID : 11111111-1111-1111-1111-111111111111
    SSID : Home | Café
    BSSID : aa:bb:cc:dd:ee:ff
    Band : 6 GHz
    Signal : 89%
    Receive rate (Mbps) : 1200.5
    Name : Wi-Fi 2
    GUID : 22222222-2222-2222-2222-222222222222
    SSID : Other
    Signal : 40%
'@ -split "`n"
$interfaces = @(Parse-Interfaces $en)
Assert ($interfaces.Count -eq 2) "Two interfaces must not be merged"
Assert ($interfaces[0].ssid -ceq 'Home | Café') "SSID was corrupted"
Assert ($interfaces[0].signal -eq 89 -and $interfaces[0].rate -eq 1200.5) "English signal/rate"
Assert ($interfaces[0].band -eq '6 GHz') "6 GHz must not be inferred from a channel"
$fr = @'
    Nom : Wi-Fi
    GUID : 11111111-1111-1111-1111-111111111111
    SSID : Maison
    Réception (Mbits/s) : 866,7
    Bande : 5 GHz
'@ -split "`n"
Assert ((@(Parse-Interfaces $fr))[0].rate -eq 866.7) "French decimal rate"
$profiles = @(Parse-Profiles @('    All User Profile : Home | Café', '    Current User Profile : Work', '    Profil Tous les utilisateurs : Maison'))
Assert ($profiles.Count -eq 3 -and $profiles -ccontains 'Home | Café') "EN/FR profiles"
$networks = @'
SSID 1 : Home | Café
    Authentication : WPA2-Personal
    Signal : 45%
    Signal : 89%
SSID 2 : Open
    Authentication : Open
    Signal : 72%
SSID 3 : open
    Authentication : WPA2-Personal
    Signal : 42%
SSID 4 :
'@ -split "`n"
$list = @(Parse-Networks $networks $profiles 'Home | Café')
Assert ($list[0].connected -and $list[0].signal -eq 89) "Connected first and strongest BSSID"
Assert (@($list | Where-Object { $_.ssid -ceq 'Open' -and -not $_.secured }).Count -eq 1) "Open security"
Assert (@($list | Where-Object { $_.ssid -ceq 'open' -and $_.secured }).Count -eq 1) "Case-sensitive SSIDs"
Assert (@($list | Where-Object { $_.ssid -ceq 'Maison' -and -not $_.available }).Count -eq 1) "Saved out-of-range networks"
Assert (@(Parse-Networks @() @() 'Still connected')[0].connected) "Connected network survives an empty scan"
$parts = Read-Action '["connect","Café | \"quoted\"","pass|word\\123"]'
Assert ($parts[1] -ceq 'Café | "quoted"' -and $parts[2] -ceq 'pass|word\123') "Structured action round trip"
Assert ((Read-Action 'connect|Legacy')[1] -eq 'Legacy') "Legacy compatibility"
$invalid = $false
try { $null = Read-Action '["connect",42,""]' } catch { $invalid = $true }
Assert $invalid "Invalid action types must fail"
[xml]$xml = New-ProfileXml 'Café & < >' 'pass|&word'
Assert ($xml.WLANProfile.SSIDConfig.SSID.name -ceq 'Café & < >') "XML escaping"
$expected = ([Text.Encoding]::UTF8.GetBytes('Café & < >') | ForEach-Object { '{0:X2}' -f $_ }) -join ''
Assert ($xml.WLANProfile.SSIDConfig.SSID.hex -eq $expected) "SSID hex must encode UTF-8 bytes"
Assert ($xml.WLANProfile.MSM.security.sharedKey.keyMaterial -ceq 'pass|&word') "Key escaping"
$invalid = $false
try { $null = New-ProfileXml ('é' * 17) 'password' } catch { $invalid = $true }
Assert $invalid "SSID limit is bytes, not characters"
Write-Output 'Wi-Fi provider fixtures passed (no network settings changed).'
