# Prints the weather as JSON for the Winarchy weather applet.
# Settings from applet.toml arrive as WINARCHY_APPLET_* environment variables.
$ErrorActionPreference = "Stop"
$lat = $env:WINARCHY_APPLET_LATITUDE
$lon = $env:WINARCHY_APPLET_LONGITUDE
$place = $env:WINARCHY_APPLET_PLACE
if (-not $lat -or -not $lon) { throw "latitude and longitude are required in [settings]" }

function Condition([int]$code) {
  if ($code -eq 0) { return "sun" }
  if ($code -le 3 -or ($code -ge 45 -and $code -le 48)) { return "cloud" }
  if (($code -ge 71 -and $code -le 77) -or $code -eq 85 -or $code -eq 86) { return "snow" }
  return "rain"
}

$url = "https://api.open-meteo.com/v1/forecast?latitude=$lat&longitude=$lon" +
  "&current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code" +
  "&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=4"
$r = Invoke-RestMethod -Uri $url -TimeoutSec 15

$days = @()
for ($i = 1; $i -lt $r.daily.time.Count; $i++) {
  $days += [ordered]@{
    name = ([datetime]$r.daily.time[$i]).ToString("dddd", [Globalization.CultureInfo]::InvariantCulture).ToUpper()
    condition = Condition $r.daily.weather_code[$i]
    max = [int][math]::Round($r.daily.temperature_2m_max[$i])
    min = [int][math]::Round($r.daily.temperature_2m_min[$i])
  }
}
[ordered]@{
  place = $place.ToUpper()
  temperature = [int][math]::Round($r.current.temperature_2m)
  feels = [int][math]::Round($r.current.apparent_temperature)
  wind = [int][math]::Round($r.current.wind_speed_10m)
  humidity = [int]$r.current.relative_humidity_2m
  condition = Condition $r.current.weather_code
  days = $days
} | ConvertTo-Json -Compress -Depth 4
