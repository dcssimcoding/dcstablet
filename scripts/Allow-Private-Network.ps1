param([string]$ExecutablePath = (Join-Path $PSScriptRoot '..\dcs-tablet.exe'), [switch]$Remove)
$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run from Administrator PowerShell.' }
$name = 'DCS Tablet private LAN'
if ($Remove) { Get-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue | Remove-NetFirewallRule; return }
$program = (Resolve-Path -LiteralPath $ExecutablePath).Path
if (-not (Test-Path -LiteralPath $program -PathType Leaf)) { throw 'Executable not found.' }
Get-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue | Remove-NetFirewallRule
New-NetFirewallRule -DisplayName $name -Direction Inbound -Action Allow -Program $program -Profile Private -RemoteAddress LocalSubnet | Out-Null
Write-Host "Private local-subnet access enabled for $program"
