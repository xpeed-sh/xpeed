param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (Test-Path -LiteralPath "$projectRoot\.tools\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
    $env:PATH = "$projectRoot\.tools\cargo\bin;C:\msys64\mingw64\bin;$env:PATH"
}
if (-not $SkipBuild) {
    cargo build --release --locked -p xpeed-server-monitor-extension --bin xpeed-server-monitor-extension
    if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de la extensión' }
}
$metadataJson = cargo metadata --locked --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'No se pudieron consultar las licencias del adaptador' }
$metadata = $metadataJson | ConvertFrom-Json
$adapter = $metadata.packages | Where-Object { $_.name -eq 'xpeed-server-monitor-extension' }
$dependencyIds = [System.Collections.Generic.HashSet[string]]::new()
$pending = [System.Collections.Generic.Queue[string]]::new()
$pending.Enqueue($adapter.id)
while ($pending.Count -gt 0) {
    $dependencyId = $pending.Dequeue()
    if (-not $dependencyIds.Add($dependencyId)) { continue }
    $node = $metadata.resolve.nodes | Where-Object { $_.id -eq $dependencyId }
    foreach ($dependency in $node.dependencies) { $pending.Enqueue($dependency) }
}
foreach ($base in @('extensions\packages', 'dist\extensions\packages')) {
    $package = Join-Path $projectRoot "$base\xpeed.server-monitor"
    New-Item -ItemType Directory -Force "$package\bin" | Out-Null
    Copy-Item -LiteralPath "$projectRoot\target\release\xpeed-server-monitor-extension.exe" -Destination "$package\bin\xpeed-server-monitor-extension.exe" -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\server-monitor\extension.json" -Destination $package -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\server-monitor\README.md" -Destination $package -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\server-monitor\README.es.md" -Destination $package -Force
    foreach ($document in @('VERSION', 'CHANGELOG.md')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "extensions/server-monitor/$document") -Destination $package -Force
    }
    Copy-Item -LiteralPath "$projectRoot\LICENSE" -Destination $package -Force
    $notices = @()
    foreach ($dependency in $metadata.packages) {
        if (-not $dependency.source -or -not $dependencyIds.Contains($dependency.id)) { continue }
        $notices += "$($dependency.name) $($dependency.version): $($dependency.license)"
        $licenseDirectory = Join-Path $package "licenses\$($dependency.name)-$($dependency.version)"
        New-Item -ItemType Directory -Force $licenseDirectory | Out-Null
        Get-ChildItem -LiteralPath (Split-Path -Parent $dependency.manifest_path) -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYRIGHT|NOTICE|COPYING)([-_.]|$)' } | ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination $licenseDirectory -Force
        }
    }
    $notices | Set-Content -LiteralPath (Join-Path $package 'THIRD_PARTY.txt') -Encoding UTF8
}
New-Item -ItemType Directory -Force "$projectRoot\dist\extensions" | Out-Null
Copy-Item -LiteralPath "$projectRoot\extensions\catalog.json" -Destination "$projectRoot\dist\extensions\catalog.json" -Force
Write-Output 'Extensión lista: extensions\packages\xpeed.server-monitor. Para preparar su proyecto Git usa scripts/prepare-git-repositories.ps1.'
