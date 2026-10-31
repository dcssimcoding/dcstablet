param(
    [string]$ReleaseExecutable = 'target\release\dcs-tablet.exe',
    [string]$HookExecutable = 'target\debug\dcs-tablet.exe'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$release = (Resolve-Path (Join-Path $root $ReleaseExecutable)).Path
$hooks = (Resolve-Path (Join-Path $root $HookExecutable)).Path
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('dcs-tablet-package-' + [Guid]::NewGuid())
try {
    & $release --verify-source $root
    if ($LASTEXITCODE -ne 0) { throw 'Matching release source was rejected.' }
    New-Item -ItemType Directory $temporary | Out-Null
    foreach ($name in @('src','web','dcs','Cargo.toml','Cargo.lock','build.rs','build_support.rs')) {
        Copy-Item (Join-Path $root $name) $temporary -Recurse
    }
    [IO.File]::AppendAllText((Join-Path $temporary 'src\main.rs'), "`n// mismatching source fixture`n")
    try { & $release --verify-source $temporary 2>$null }
    catch { if ($LASTEXITCODE -eq 0) { throw } }
    if ($LASTEXITCODE -eq 0) { throw 'Stale same-version executable was accepted.' }
    $hookRejected = $false
    try { & (Join-Path $root 'scripts\Package.ps1') -ExecutablePath $hooks }
    catch {
        if ($_.Exception.Message -notmatch 'without test hooks') { throw }
        $hookRejected = $true
    }
    if (-not $hookRejected) { throw 'Test-hook packaging was accepted.' }
    Write-Host 'PASS: matching source accepted; same-version stale source and test-hook package rejected.'
} finally {
    Remove-Item $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
