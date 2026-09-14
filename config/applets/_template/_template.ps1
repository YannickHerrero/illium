# Provider template. Print one JSON object on stdout; keys are matched to the
# fields of the `Data` struct declared in view.slint (extra keys are ignored).
# The first argument, when present, is the action requested by the view.
param([string]$Action = "")
$greeting = $env:WINARCHY_APPLET_GREETING
[ordered]@{
  message = "$greeting from PowerShell"
  count = (Get-Process).Count
  action = $Action
} | ConvertTo-Json -Compress
