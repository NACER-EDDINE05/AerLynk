$ErrorActionPreference = 'Stop'

$program = $env:AERLYNK_PROGRAM_PATH
if (-not $program) {
    $program = (Get-Process -Id $PID -ErrorAction SilentlyContinue).Path
}
if (-not $program) {
    $program = (Get-Command aerlynk -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
}
if (-not $program) {
    throw 'AerLynk executable path is required.'
}

$allowRule = 'AerLynk (inbound LAN)'
$blockRules = @(Get-NetFirewallRule -Direction Inbound -Action Block -ErrorAction SilentlyContinue | Where-Object { $_.Program -and $_.Program -like '*aerlynk*' })
foreach ($rule in $blockRules) {
    try {
        Remove-NetFirewallRule -DisplayName $rule.DisplayName -ErrorAction SilentlyContinue
    } catch {}
}

try {
    Remove-NetFirewallRule -DisplayName $allowRule -ErrorAction SilentlyContinue
} catch {}

New-NetFirewallRule -DisplayName $allowRule `
    -Description 'Allows AerLynk to accept incoming file transfers from phones on the local network' `
    -Direction Inbound -Program $program -Protocol TCP -Action Allow `
    -Profile Private,Public -Enabled True | Out-Null

Write-Output "AerLynk firewall rule ready"
