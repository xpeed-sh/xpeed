param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (Test-Path -LiteralPath (Join-Path $projectRoot '.tools\cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
    $env:PATH = "$(Join-Path $projectRoot '.tools\cargo\bin');$env:PATH"
}
if (Test-Path -LiteralPath 'C:\msys64\mingw64\bin\gcc.exe') {
    $env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
}
if (-not $SkipBuild) {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'La compilación falló' }
}
$appVersion = (Get-Content -LiteralPath (Join-Path $projectRoot 'VERSION') -Raw).Trim()
if ($appVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'VERSION must use x.y.z' }
$distribution = Join-Path $projectRoot 'dist'
New-Item -ItemType Directory -Force -Path $distribution | Out-Null
$releaseSource = Join-Path $projectRoot 'target\release\xpeed.exe'
$versionedDestination = Join-Path $distribution "Xpeed-$appVersion.exe"
if (-not (Test-Path -LiteralPath $versionedDestination) -or (Get-FileHash -LiteralPath $releaseSource).Hash -ne (Get-FileHash -LiteralPath $versionedDestination).Hash) {
    Copy-Item -LiteralPath $releaseSource -Destination $versionedDestination -Force
}
try {
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\xpeed.exe') -Destination (Join-Path $distribution 'Xpeed.exe') -Force
} catch {
    Write-Warning "Xpeed.exe is open. Use Xpeed-$appVersion.exe; existing sessions remain active."
}
Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $distribution -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $distribution -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD_PARTY.md') -Destination $distribution -Force
foreach ($document in @('CHANGELOG.md', 'VERSION', 'CREDITS.md', 'CONTRIBUTING.md')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $document) -Destination $distribution -Force
}
$technicalDocs = Join-Path $distribution 'docs'
New-Item -ItemType Directory -Force -Path $technicalDocs | Out-Null
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'docs') -File -Filter '*.md' | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $technicalDocs -Force }
$catalogDirectory = Join-Path $distribution 'extensions'
New-Item -ItemType Directory -Force -Path $catalogDirectory | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'extensions\catalog.json') -Destination $catalogDirectory -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'extensions\catalog.git.example.json') -Destination $catalogDirectory -Force
$monitorDocs = Join-Path $catalogDirectory 'server-monitor'
New-Item -ItemType Directory -Force -Path $monitorDocs | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'extensions\server-monitor\README.md') -Destination $monitorDocs -Force
$additionalNotices = Join-Path $distribution 'licenses\additional'
New-Item -ItemType Directory -Force -Path $additionalNotices | Out-Null
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'assets\licenses') -File | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $additionalNotices -Force
}
if (Test-Path -LiteralPath (Join-Path $projectRoot 'assets\preview.png')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $distribution 'assets') | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\preview.png') -Destination (Join-Path $distribution 'assets\preview.png') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\preview-light.png') -Destination (Join-Path $distribution 'assets\preview-light.png') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\preview-terminal.png') -Destination (Join-Path $distribution 'assets\preview-terminal.png') -Force
}
$previousErrorPreference = $ErrorActionPreference
try {
    # Cargo writes download progress to stderr; Windows PowerShell must not
    # treat a successful dependency download as a packaging failure.
    $ErrorActionPreference = 'Continue'
    $metadataJson = cargo metadata --locked --format-version 1
    $metadataExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $previousErrorPreference
}
if ($metadataExitCode -ne 0) { throw 'No se pudo consultar las dependencias' }
$metadata = $metadataJson | ConvertFrom-Json
$dependencyList = @()
foreach ($package in $metadata.packages) {
    if (-not $package.source) { continue }
    $dependencyList += "$($package.name) $($package.version): $($package.license)"
    $sourceDirectory = Split-Path -Parent $package.manifest_path
    $licenseDirectory = Join-Path $distribution "licenses\$($package.name)-$($package.version)"
    $licenseFiles = Get-ChildItem -LiteralPath $sourceDirectory -File -Recurse | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYRIGHT|NOTICE|OFL|UFL)([-_.]|$)' -or $_.Name -match '(license|licence)\.(txt|md)$' }
    foreach ($licenseFile in $licenseFiles) {
        $relative = $licenseFile.FullName.Substring($sourceDirectory.Length).TrimStart('\', '/')
        $destination = Join-Path $licenseDirectory $relative
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
        Copy-Item -LiteralPath $licenseFile.FullName -Destination $destination -Force
    }
}
$dependencyList | Set-Content -LiteralPath (Join-Path $distribution 'DEPENDENCIES.txt') -Encoding UTF8
$portableDirectory = Join-Path $distribution "portable\Xpeed-$appVersion"
New-Item -ItemType Directory -Force -Path $portableDirectory | Out-Null
Copy-Item -LiteralPath (Join-Path $distribution "Xpeed-$appVersion.exe") -Destination (Join-Path $portableDirectory 'Xpeed.exe') -Force
foreach ($document in @('LICENSE', 'THIRD_PARTY.md', 'DEPENDENCIES.txt', 'README.md', 'CHANGELOG.md', 'VERSION', 'CREDITS.md', 'CONTRIBUTING.md')) {
    Copy-Item -LiteralPath (Join-Path $distribution $document) -Destination $portableDirectory -Force
}
foreach ($folder in @('licenses', 'extensions', 'assets', 'docs')) {
    $source = Join-Path $distribution $folder
    if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination $portableDirectory -Recurse -Force }
}
Write-Output "Carpeta portátil para compartir: $portableDirectory"
Write-Output "Xpeed listo: $distribution\Xpeed-$appVersion.exe"
