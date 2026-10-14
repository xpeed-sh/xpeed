param([switch]$ResolveOnly)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -LiteralPath (Join-Path $projectRoot 'VERSION') -Raw).Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid VERSION format.' }
$candidates = @(
    (Join-Path $projectRoot 'dist/Xpeed.exe'),
    (Join-Path $projectRoot "dist/Xpeed-$version.exe"),
    (Join-Path $projectRoot 'target/release/xpeed.exe')
)
$executable = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    ForEach-Object { Get-Item -LiteralPath $_ } |
    Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
if ($executable) {
    if ($ResolveOnly) { Write-Output $executable.FullName; return }
    # Run an immutable copy so neither Cargo nor packaging has to replace an
    # executable held open by the application.
    $hash = (Get-FileHash -LiteralPath $executable.FullName -Algorithm SHA256).Hash
    $launchDirectory = Join-Path $projectRoot 'dist/launch'
    New-Item -ItemType Directory -Force -Path $launchDirectory | Out-Null
    $launchPath = Join-Path $launchDirectory "Xpeed-$version-$hash.exe"
    if (-not (Test-Path -LiteralPath $launchPath)) {
        Copy-Item -LiteralPath $executable.FullName -Destination $launchPath
    }
    Start-Process -FilePath $launchPath -WorkingDirectory $projectRoot
} elseif ($ResolveOnly) {
    Write-Output 'No compiled executable; launcher will use scripts/dev.ps1 -Release.'
} else {
    & (Join-Path $PSScriptRoot 'dev.ps1') -Release
    exit $LASTEXITCODE
}
