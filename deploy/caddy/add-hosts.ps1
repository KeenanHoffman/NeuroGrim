# deploy/caddy/add-hosts.ps1
#
# Adds the *.localhost entries to Windows' hosts file so Rust/Python/Go
# stdlib resolvers (which don't special-case `.localhost` like curl does)
# can reach the Caddy dev stack. Admin required.
#
# Usage (elevated PowerShell):
#   powershell -ExecutionPolicy Bypass -File deploy/caddy/add-hosts.ps1

$ErrorActionPreference = "Stop"

$hostsFile = "$env:SystemRoot\System32\drivers\etc\hosts"
$entries = @(
    "127.0.0.1 neurogrim-local.localhost",
    "127.0.0.1 neurogrim-external.localhost",
    "127.0.0.1 webhooks.localhost"
)

$existing = Get-Content $hostsFile -ErrorAction Stop
$toAdd = @()
foreach ($entry in $entries) {
    if ($existing -contains $entry) {
        Write-Host ("already present: " + $entry)
    } else {
        $toAdd += $entry
    }
}
if ($toAdd.Count -eq 0) {
    Write-Host "hosts file already has every entry, nothing to do"
    exit 0
}

Add-Content -Path $hostsFile -Value ""
Add-Content -Path $hostsFile -Value "# Added by NeuroGrim deploy/caddy/add-hosts.ps1"
foreach ($entry in $toAdd) {
    Add-Content -Path $hostsFile -Value $entry
    Write-Host ("added: " + $entry)
}
Write-Host ""
Write-Host "Done. Flush the DNS cache if you want immediate effect:"
Write-Host "  ipconfig /flushdns"
