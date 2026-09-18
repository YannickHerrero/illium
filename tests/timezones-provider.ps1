$ErrorActionPreference = 'Stop'
$provider = Join-Path $PSScriptRoot '../config/applets/timezones/timezones.ps1'
function Assert($condition, $message) { if (-not $condition) { throw $message } }
$previous = $env:WINARCHY_APPLET_ZONES
try {
    $env:WINARCHY_APPLET_ZONES = ''
    $winter = & $provider refresh -At '2026-01-01T23:30:00Z' -LocalZoneId 'Romance Standard Time' | ConvertFrom-Json
    Assert ($winter.rows.Count -eq 4) 'Four default rows'
    Assert ($winter.rows[0].time -eq '00:30') 'Local date rollover'
    Assert ($winter.rows[1].time -eq '23:30' -and $winter.rows[1].zone -eq 'UTC+00:00') 'London winter'
    Assert ($winter.rows[2].time -eq '00:30' -and $winter.rows[2].zone -eq 'UTC+01:00') 'Paris winter'
    Assert ($winter.rows[3].time -eq '08:30') 'Tokyo date rollover'
    Assert ($winter.position -eq 0.5) 'Minute cursor'
    foreach ($row in $winter.rows) {
        Assert ($row.cells.Count -eq 24) '24 shared columns'
        Assert (@($row.cells | Where-Object current).Count -eq 1) 'One current cell'
    }
    Assert ($winter.rows[2].cells[0].label -eq "Fri`n02") 'Midnight date label'
    Assert ($winter.rows[3].cells[0].daytime) 'Daytime tint'
    Assert ($winter.rows[2].cells[0].night) 'Night tint'
    $summer = & $provider -At '2026-07-01T12:00:00Z' -LocalZoneId 'UTC' | ConvertFrom-Json
    Assert ($summer.rows[1].zone -eq 'UTC+01:00') 'London DST'
    Assert ($summer.rows[2].zone -eq 'UTC+02:00') 'Paris DST'
    Assert ($summer.rows[3].zone -eq 'UTC+09:00') 'Tokyo has no DST'
    $spring = & $provider -At '2026-03-29T00:00:00Z' -LocalZoneId 'UTC' | ConvertFrom-Json
    Assert ($spring.rows[1].cells[1].label -eq '2') 'Skip missing spring hour'
    $autumn = & $provider -At '2026-10-25T00:00:00Z' -LocalZoneId 'UTC' | ConvertFrom-Json
    Assert ($autumn.rows[1].cells[0].label -eq '1' -and $autumn.rows[1].cells[1].label -eq '1') 'Repeated autumn hour'
    $env:WINARCHY_APPLET_ZONES = 'Delhi|India Standard Time'
    $custom = & $provider -At '2026-07-01T12:00:00Z' -LocalZoneId 'UTC' | ConvertFrom-Json
    Assert ($custom.rows[1].time -eq '17:30' -and $custom.rows[1].cells[0].label -eq '05:30') 'Fractional offset'
    Write-Output 'Timezone provider checks passed'
} finally {
    $env:WINARCHY_APPLET_ZONES = $previous
}
