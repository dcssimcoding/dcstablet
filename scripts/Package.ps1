param([string]$Version = '0.1.1', [string]$ExecutablePath)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $binary = if ($ExecutablePath) { (Resolve-Path -LiteralPath $ExecutablePath).Path } else { Join-Path $root 'target\release\dcs-tablet.exe' }
    if (-not (Test-Path $binary)) { throw 'Run cargo build --release --locked first.' }
    & python (Join-Path $root 'scripts\Check-ReleasePrivacy.py') $binary
    if ($LASTEXITCODE -ne 0) { throw 'Executable contains private build paths or debug symbols.' }
    $reported = & $binary --version
    if ($LASTEXITCODE -ne 0 -or $reported -notmatch ('^DCS Tablet v' + [regex]::Escape($Version) + ' ')) { throw 'Binary version does not match package version.' }
    $buildInfoText = & $binary --build-info
    if ($LASTEXITCODE -ne 0) { throw 'Executable build metadata is unavailable.' }
    $buildInfo = ($buildInfoText -join "`n") | ConvertFrom-Json
    if ($buildInfo.default_features -ne $true -or $buildInfo.test_hooks -ne $false -or $buildInfo.target -ne 'x86_64-pc-windows-msvc') { throw 'Package requires a default production Windows x64 build without test hooks.' }
    & $binary --verify-source $root
    if ($LASTEXITCODE -ne 0) { throw 'Binary does not match this source checkout and dependency graph.' }
    & python (Join-Path $root 'scripts\Build-Guide.py') --check
    if ($LASTEXITCODE -ne 0) { throw 'Offline guide is stale. Rebuild it before packaging.' }
    $destination = Join-Path $root ('dist\dcs-tablet-' + $Version + '-windows-x64')
    if (Test-Path $destination) { throw "Package directory already exists: $destination. Preserve or move it before packaging again." }
    New-Item -ItemType Directory -Path $destination | Out-Null
    Copy-Item $binary (Join-Path $destination 'dcs-tablet.exe')
    $buildInfo | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $destination 'BUILD-INFO.json')
    foreach ($name in @('README.md','START-HERE.html','CONTRIBUTING.md','CHANGELOG.md','LICENSE','THIRD-PARTY-NOTICES.md','Start-Demo.cmd','Start-DCS-Tablet.cmd','Restore-Display.cmd')) { Copy-Item (Join-Path $root $name) $destination -Recurse }
    $guideFolder = Join-Path $destination 'docs'
    New-Item -ItemType Directory $guideFolder | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $root 'docs') -File -Filter '*.md' | Copy-Item -Destination $guideFolder
    Copy-Item (Join-Path $root 'docs\images') $guideFolder -Recurse
    New-Item -ItemType Directory (Join-Path $destination 'scripts') | Out-Null
    Copy-Item (Join-Path $root 'scripts\Allow-Private-Network.ps1') (Join-Path $destination 'scripts')
    $metadataText = & cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Cargo dependency metadata failed.' }
    $metadata = ($metadataText -join "`n") | ConvertFrom-Json
    $ids = @{}
    foreach ($node in $metadata.resolve.nodes) { $ids[$node.id] = $true }
    $licenses = Join-Path $destination 'third-party-licenses'
    New-Item -ItemType Directory $licenses | Out-Null
    $index = @('# Dependency licenses', '', 'Source license texts collected from the locked Windows dependency graph.', '')
    foreach ($package in $metadata.packages | Sort-Object name,version) {
        if ($package.name -eq 'dcs-tablet' -or -not $ids.ContainsKey($package.id)) { continue }
        $source = Split-Path $package.manifest_path -Parent
        $files = @(Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' })
        foreach ($subdirectory in @('LICENSES','licenses','LICENSE','license')) { if (Test-Path (Join-Path $source $subdirectory) -PathType Container) { $files += @(Get-ChildItem -LiteralPath (Join-Path $source $subdirectory) -Recurse -File) } }
        if ($package.license_file) { $files += Get-Item (Join-Path $source $package.license_file) }
        $supplement = Join-Path $root ('licenses\' + $package.name + '-' + $package.version)
        if (Test-Path $supplement) { $files += @(Get-ChildItem -LiteralPath $supplement -File) }
        $files = @($files | Sort-Object FullName -Unique)
        if ($files.Count -eq 0) { throw "No license text found for $($package.name) $($package.version). Review before distributing." }
        $folder = Join-Path $licenses ($package.name + '-' + $package.version)
        New-Item -ItemType Directory $folder | Out-Null
        $counter = 0
        foreach ($file in $files) { $counter++; Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $folder ("$counter-" + $file.Name)) }
        $index += "- $($package.name) $($package.version): $($package.license)"
    }
    $index | Set-Content -Encoding UTF8 (Join-Path $licenses 'INDEX.md')
    # Crate archives may carry timestamps outside ZIP's supported DOS date range.
    $timestamp = [DateTime]::UtcNow
    if ($env:SOURCE_DATE_EPOCH) { $timestamp = [DateTimeOffset]::FromUnixTimeSeconds([long]$env:SOURCE_DATE_EPOCH).UtcDateTime }
    if ($timestamp.Year -lt 1980) { $timestamp = [DateTime]::new(1980,1,1,0,0,0,[DateTimeKind]::Utc) }
    foreach ($entry in Get-ChildItem -LiteralPath $destination -Recurse) { $entry.LastWriteTimeUtc = $timestamp }
    $archive = $destination + '.zip'
    Compress-Archive -Path (Join-Path $destination '*') -DestinationPath $archive
    & python (Join-Path $root 'scripts\Check-ReleasePrivacy.py') $archive
    if ($LASTEXITCODE -ne 0) {
        Remove-Item -LiteralPath $archive
        throw 'Release ZIP failed privacy checks; archive removed.'
    }
    $hash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -Encoding ASCII ($archive + '.sha256')
    Write-Host "Package: $archive"
} finally { Pop-Location }
