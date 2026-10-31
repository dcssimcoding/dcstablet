$ErrorActionPreference = 'Stop'
# Apply to dependencies as well as the application. Never put personal prefixes
# into repository files or command output.
$root = Split-Path $PSScriptRoot -Parent
$prefixes = @($env:USERPROFILE, $env:HOME, $env:CARGO_HOME, $env:RUSTUP_HOME, $root) |
    Where-Object { $_ -and $_.Length -gt 3 } | Sort-Object -Unique | Sort-Object Length
$flags = @()
if ($env:CARGO_ENCODED_RUSTFLAGS) {
    $flags += $env:CARGO_ENCODED_RUSTFLAGS.Split([char]31)
} elseif ($env:RUSTFLAGS) {
    throw 'Use CARGO_ENCODED_RUSTFLAGS instead of RUSTFLAGS before configuring private builds.'
}
foreach ($prefix in $prefixes) {
    foreach ($form in @($prefix, $prefix.Replace('\', '/'), $prefix.Replace('/', '\')) | Select-Object -Unique) {
        $flags += '--remap-path-prefix=' + $form + '=/build'
    }
}
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
if ($env:GITHUB_ENV) {
    'CARGO_ENCODED_RUSTFLAGS=' + $env:CARGO_ENCODED_RUSTFLAGS |
        Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}
Write-Host 'Build path remapping configured.'
