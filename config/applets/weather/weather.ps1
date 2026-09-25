# Prints the weather as JSON for the Illium weather applet.
# Settings from applet.toml arrive as ILLIUM_APPLET_* environment variables.
$ErrorActionPreference = "Stop"
$lat = $env:ILLIUM_APPLET_LATITUDE
$lon = $env:ILLIUM_APPLET_LONGITUDE
$place = $env:ILLIUM_APPLET_PLACE
if (-not $lat -or -not $lon) { throw "latitude and longitude are required in [settings]" }
$culture = [Globalization.CultureInfo]::InvariantCulture

# WMO weather codes, split into a monochrome icon name and a short label.
function Condition([int]$code, [bool]$day) {
  if ($code -eq 0) { if ($day) { return "sun" } else { return "moon" } }
  if ($code -le 2) { if ($day) { return "partly" } else { return "partly-night" } }
  if ($code -eq 3) { return "cloud" }
  if ($code -le 48) { return "fog" }
  if (($code -ge 71 -and $code -le 77) -or $code -eq 85 -or $code -eq 86) { return "snow" }
  if ($code -ge 95) { return "storm" }
  return "rain"
}
function Label([int]$code) {
  switch ($code) {
    0 { "Clear sky" }
    1 { "Mainly clear" }
    2 { "Partly cloudy" }
    3 { "Overcast" }
    { $_ -in 45, 48 } { "Fog" }
    { $_ -in 51, 53, 55, 56, 57 } { "Drizzle" }
    { $_ -in 61, 66, 80 } { "Light rain" }
    { $_ -in 63, 81 } { "Rain" }
    { $_ -in 65, 67, 82 } { "Heavy rain" }
    { $_ -in 71, 85 } { "Light snow" }
    { $_ -in 73, 75, 77, 86 } { "Snow" }
    95 { "Thunderstorm" }
    { $_ -in 96, 99 } { "Thunderstorm with hail" }
    default { "Unknown" }
  }
}
function Compass([double]$degrees) {
  $points = "N", "NE", "E", "SE", "S", "SW", "W", "NW"
  return $points[[int][math]::Round($degrees / 45) % 8]
}

$url = "https://api.open-meteo.com/v1/forecast?latitude=$lat&longitude=$lon" +
  "&current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,wind_direction_10m,weather_code,is_day" +
  "&hourly=temperature_2m,weather_code,precipitation_probability,is_day" +
  "&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max" +
  "&timezone=auto&forecast_days=7"
$r = Invoke-RestMethod -Uri $url -TimeoutSec 15

# Hourly series starts at the current hour in the location's time zone.
$nowHour = ([datetime]$r.current.time).ToString("yyyy-MM-ddTHH:00", $culture)
$start = [array]::IndexOf($r.hourly.time, $nowHour)
if ($start -lt 0) { $start = 0 }
$hours = @()
for ($i = $start; $i -lt $start + 12 -and $i -lt $r.hourly.time.Count; $i++) {
  $hours += [ordered]@{
    name = ([datetime]$r.hourly.time[$i]).ToString("HH'h'", $culture)
    condition = Condition $r.hourly.weather_code[$i] ($r.hourly.is_day[$i] -eq 1)
    temperature = [int][math]::Round($r.hourly.temperature_2m[$i])
    rain = [int]$r.hourly.precipitation_probability[$i]
  }
}

$count = [math]::Min(6, $r.daily.time.Count)
$coldest = ($r.daily.temperature_2m_min[0..($count - 1)] | Measure-Object -Minimum).Minimum
$warmest = ($r.daily.temperature_2m_max[0..($count - 1)] | Measure-Object -Maximum).Maximum
$span = [math]::Max(1, $warmest - $coldest)
$days = @()
for ($i = 0; $i -lt $count; $i++) {
  $date = [datetime]$r.daily.time[$i]
  $days += [ordered]@{
    name = if ($i -eq 0) { "TODAY" } else { $date.ToString("ddd dd", $culture).ToUpper() }
    condition = Condition $r.daily.weather_code[$i] $true
    rain = [int]$r.daily.precipitation_probability_max[$i]
    max = [int][math]::Round($r.daily.temperature_2m_max[$i])
    min = [int][math]::Round($r.daily.temperature_2m_min[$i])
    # Bar extent within the six-day range, 0..1, so the view needs no cross-row math.
    lo = [math]::Round(($r.daily.temperature_2m_min[$i] - $coldest) / $span, 3)
    hi = [math]::Round(($r.daily.temperature_2m_max[$i] - $coldest) / $span, 3)
  }
}

[ordered]@{
  place = $place.ToUpper()
  temperature = [int][math]::Round($r.current.temperature_2m)
  feels = [int][math]::Round($r.current.apparent_temperature)
  wind = [int][math]::Round($r.current.wind_speed_10m)
  wind_direction = Compass $r.current.wind_direction_10m
  humidity = [int]$r.current.relative_humidity_2m
  condition = Condition $r.current.weather_code ($r.current.is_day -eq 1)
  label = Label $r.current.weather_code
  updated = (Get-Date).ToString("HH:mm", $culture)
  hours = $hours
  days = $days
} | ConvertTo-Json -Compress -Depth 4
