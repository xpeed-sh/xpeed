param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (Test-Path -LiteralPath "$projectRoot\.tools\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
    $env:PATH = "$projectRoot\.tools\cargo\bin;C:\msys64\mingw64\bin;$env:PATH"
}
$extensionRoot = Join-Path $projectRoot 'extensions\codex'
$extensionManifest = Join-Path $extensionRoot 'Cargo.toml'
if (-not (Test-Path -LiteralPath $extensionManifest)) {
    throw 'Falta extensions\codex. Clona https://github.com/xpeed-sh/extension-codex en esa ruta.'
}
if (-not (Test-Path -LiteralPath (Join-Path $extensionRoot 'Cargo.lock'))) {
    cargo generate-lockfile --manifest-path $extensionManifest
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo crear Cargo.lock para Codex' }
}
if (-not $SkipBuild) {
    cargo build --release --locked --manifest-path $extensionManifest --target-dir "$projectRoot\target" --bin xpeed-codex-extension
    if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de la extensión' }
}
$metadataJson = cargo metadata --locked --manifest-path $extensionManifest --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'No se pudieron consultar las licencias del adaptador' }
$metadata = $metadataJson | ConvertFrom-Json
$adapter = $metadata.packages | Where-Object { $_.name -eq 'xpeed-codex-extension' }
$dependencyIds = [System.Collections.Generic.HashSet[string]]::new()
$pending = [System.Collections.Generic.Queue[string]]::new()
$pending.Enqueue($adapter.id)
while ($pending.Count -gt 0) {
    $dependencyId = $pending.Dequeue()
    if (-not $dependencyIds.Add($dependencyId)) { continue }
    $node = $metadata.resolve.nodes | Where-Object { $_.id -eq $dependencyId }
    foreach ($dependency in $node.dependencies) { $pending.Enqueue($dependency) }
}
foreach ($base in @('extensions\packages')) {
    $package = Join-Path $projectRoot "$base\xpeed.codex"
    foreach ($generated in @('licenses', 'THIRD_PARTY.txt')) {
        $generatedPath = Join-Path $package $generated
        if (Test-Path -LiteralPath $generatedPath) {
            Remove-Item -LiteralPath $generatedPath -Recurse -Force
        }
    }
    New-Item -ItemType Directory -Force "$package\bin" | Out-Null
    Copy-Item -LiteralPath "$projectRoot\target\release\xpeed-codex-extension.exe" -Destination "$package\bin\xpeed-codex-extension.exe" -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\codex\extension.json" -Destination $package -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\codex\README.md" -Destination $package -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\codex\README.es.md" -Destination $package -Force
    Copy-Item -LiteralPath "$projectRoot\extensions\codex\assets" -Destination $package -Recurse -Force
    foreach ($document in @('VERSION', 'CHANGELOG.md')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "extensions/codex/$document") -Destination $package -Force
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
Write-Output 'Extensión lista: extensions\packages\xpeed.codex. Para preparar su proyecto Git usa scripts/prepare-git-repositories.ps1.'
