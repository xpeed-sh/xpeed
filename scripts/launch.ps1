param([switch]$ResolveOnly)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -LiteralPath (Join-Path $projectRoot 'VERSION') -Raw).Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid VERSION format.' }

function Find-XpeedExecutable {
    @(
        (Join-Path $projectRoot 'dist/Xpeed.exe'),
        (Join-Path $projectRoot "dist/Xpeed-$version.exe"),
        (Join-Path $projectRoot 'target/release/xpeed.exe')
    ) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        ForEach-Object { Get-Item -LiteralPath $_ } |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
}

$executable = Find-XpeedExecutable
$sourceRoots = @('src', 'crates', 'assets') | ForEach-Object { Join-Path $projectRoot $_ } |
    Where-Object { Test-Path -LiteralPath $_ }
$sourceFiles = @('Cargo.toml', 'Cargo.lock', 'build.rs', 'VERSION') |
    ForEach-Object { Join-Path $projectRoot $_ } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }
$newestSource = @(
    $sourceFiles | ForEach-Object { Get-Item -LiteralPath $_ }
    $sourceRoots | ForEach-Object { Get-ChildItem -LiteralPath $_ -Recurse -File }
) | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
$needsBuild = -not $executable -or ($newestSource -and $newestSource.LastWriteTimeUtc -gt $executable.LastWriteTimeUtc)

if ($ResolveOnly) {
    if ($needsBuild) {
        Write-Output 'Source changes detected; launcher will rebuild target/release/xpeed.exe.'
    } else {
        Write-Output $executable.FullName
    }
    return
}

if ($needsBuild) {
    Set-Location -LiteralPath $projectRoot
    $localCargo = Join-Path $projectRoot '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
        $env:PATH = "$(Join-Path $projectRoot '.tools\cargo\bin');$env:PATH"
    }
    if (Test-Path -LiteralPath 'C:\msys64\mingw64\bin\gcc.exe') {
        $env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $executable = Find-XpeedExecutable
}

if (-not $executable) {
    throw 'Release build completed without producing target/release/xpeed.exe.'
}

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
