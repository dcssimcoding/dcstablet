# Exercises the production child ownership/log-pipe path using a synthetic demo.
# This test-only entry point never starts DCS, touches profiles, or configures displays.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$executable = Join-Path $root 'target\debug\dcs-tablet.exe'
$directory = Join-Path ([IO.Path]::GetTempPath()) ('dcs-tablet-logging-' + [Guid]::NewGuid())
$process = $null
$ownedPid = $null
try {
    New-Item -ItemType Directory $directory | Out-Null
    $process = Start-Process $executable -ArgumentList @('--test-supervised-demo','18196',('"' + $directory + '"')) -PassThru -NoNewWindow
    $null = $process.Handle
    $ready = $false
    for ($n=0; $n -lt 100; $n++) {
        if ($process.HasExited) { throw 'Owned demo exited during startup.' }
        try {
            $status = Invoke-RestMethod 'http://127.0.0.1:18196/health' -TimeoutSec 1
            if ($status.mode -eq 'demo' -and $status.test_hooks -and (Test-Path (Join-Path $directory 'owned-ready.json'))) {
                $owner = Get-CimInstance Win32_Process -Filter ("ProcessId=" + $status.process_id)
                if ($owner.ParentProcessId -eq $process.Id) { $ownedPid = $status.process_id; $ready = $true; break }
            }
        } catch { }
        Start-Sleep -Milliseconds 100
    }
    if (-not $ready) { throw 'Owned demo never became healthy.' }
    Invoke-RestMethod 'http://127.0.0.1:18196/shutdown' -Method Post -TimeoutSec 2 | Out-Null
    if (-not $process.WaitForExit(10000)) { throw 'Owned demo/log readers did not shut down normally.' }
    $process.Refresh()
    if ($process.ExitCode -ne 0) { throw 'Owned demo returned an error.' }
    $log = Get-Content (Join-Path $directory 'stream.log') -Raw
    if ($log -notmatch 'Session' -or $log -notmatch 'DEMO' -or $log -notmatch 'Rust streamer stopped') { throw 'Owned child output was not fully drained.' }
    Write-Host 'PASS: owned demo startup, bounded log pipes, clean child exit and reader joins.'
} finally {
    if ($ownedPid) {
        $owner = Get-CimInstance Win32_Process -Filter ("ProcessId=" + $ownedPid) -ErrorAction SilentlyContinue
        if ($owner -and $process -and $owner.ParentProcessId -eq $process.Id) { Stop-Process -Id $ownedPid -ErrorAction SilentlyContinue }
    }
    if ($process -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    Remove-Item $directory -Recurse -Force -ErrorAction SilentlyContinue
}
