# Provider template. Print one JSON object on stdout; the page receives it
# unchanged through illium.on('data', ...).
# The first argument, when present, is the action requested by the page.
param([string]$Action = "")
$greeting = $env:ILLIUM_APPLET_GREETING
[ordered]@{
  message = "$greeting from PowerShell"
  count = (Get-Process).Count
  action = $Action
} | ConvertTo-Json -Compress
