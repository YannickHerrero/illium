param(
    [string]$Action,
    [DateTimeOffset]$At = [DateTimeOffset]::UtcNow,
    [string]$LocalZoneId = [TimeZoneInfo]::Local.Id
)
$ErrorActionPreference = 'Stop'
$invariant = [Globalization.CultureInfo]::InvariantCulture
$now = $At.ToUniversalTime()
$localZone = [TimeZoneInfo]::FindSystemTimeZoneById($LocalZoneId)
$local = [TimeZoneInfo]::ConvertTime($now, $localZone)
# Columns are shared instants; the current local hour fixes today's origin.
# Convert each instant independently, including across DST transitions.
$start = $now.AddHours(-$local.Hour).AddMinutes(-$local.Minute).AddSeconds(-$local.Second)
$zones = $env:ILLIUM_APPLET_ZONES
if ([string]::IsNullOrWhiteSpace($zones)) {
    $zones = 'London|GMT Standard Time;Paris|Romance Standard Time;Tokyo|Tokyo Standard Time'
}
$entries = @(@{ name = 'Local'; zone = $localZone })
foreach ($entry in $zones.Split(';')) {
    $parts = $entry.Split('|', 2)
    if ($parts.Count -ne 2) { throw "Expected City|Windows time zone identifier" }
    $entries += @{ name = $parts[0]; zone = [TimeZoneInfo]::FindSystemTimeZoneById($parts[1]) }
}
$rows = @(foreach ($entry in $entries) {
    $time = [TimeZoneInfo]::ConvertTime($now, $entry.zone)
    $delta = ($time.Offset - $local.Offset).TotalHours
    $offset = if ($delta -gt 0) { '+' + $delta.ToString('0.##', $invariant) + 'h' } else { $delta.ToString('0.##', $invariant) + 'h' }
    $cells = @(for ($i = 0; $i -lt 24; $i++) {
        $at = [TimeZoneInfo]::ConvertTime($start.AddHours($i), $entry.zone)
        @{
            label = if ($at.Hour -eq 0 -and $at.Minute -eq 0) { $at.ToString("ddd`ndd", $invariant) } elseif ($at.Minute -ne 0) { $at.ToString('HH:mm', $invariant) } else { $at.ToString('%H', $invariant) }
            daytime = $at.Hour -ge 8 -and $at.Hour -lt 18
            night = $at.Hour -lt 6 -or $at.Hour -ge 23
            current = $i -eq $local.Hour
        }
    })
    @{
        name = $entry.name
        zone = 'UTC' + $time.ToString('zzz', $invariant)
        time = $time.ToString('HH:mm', $invariant)
        detail = $time.ToString('ddd dd MMM', $invariant) + '  ' + $offset
        cells = $cells
    }
})
@{ rows = $rows; position = $local.Hour + $local.Minute / 60.0 } | ConvertTo-Json -Depth 6 -Compress
