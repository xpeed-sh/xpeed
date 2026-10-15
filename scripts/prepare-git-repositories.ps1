param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (Test-Path -LiteralPath (Join-Path $projectRoot '.tools/cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
    $env:PATH = "$projectRoot\.tools\cargo\bin;$env:PATH"
}
if (Test-Path -LiteralPath 'C:\msys64\mingw64\bin\gcc.exe') {
    $env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
}
if (-not $SkipBuild) {
    cargo build --release --locked -p xpeed-codex-extension -p xpeed-server-monitor-extension
    if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de las extensiones' }
}
foreach ($entry in @(
    @{Name='codex'; Id='xpeed.codex'; Binary='xpeed-codex-extension'},
    @{Name='server-monitor'; Id='xpeed.server-monitor'; Binary='xpeed-server-monitor-extension'}
)) {
    $source = Join-Path $projectRoot "extensions\$($entry.Name)"
    $destination = Join-Path $projectRoot "dist\git-repositories\$($entry.Id)"
    if (Test-Path -LiteralPath (Join-Path $destination '.git')) {
        throw "El destino ya contiene un repositorio Git. Conserva sus cambios antes de regenerar: $destination"
    }
    $version = (Get-Content -LiteralPath (Join-Path $source 'VERSION') -Raw).Trim()
    $manifest = Get-Content -LiteralPath (Join-Path $source 'extension.json') -Raw | ConvertFrom-Json
    $sourceCargo = Get-Content -LiteralPath (Join-Path $source 'Cargo.toml') -Raw
    $cargoVersion = [regex]::Match($sourceCargo, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if ($version -notmatch '^\d+\.\d+\.\d+$' -or $manifest.version -ne $version -or $cargoVersion -ne $version -or $manifest.id -ne $entry.Id) {
        throw "VERSION, Cargo.toml y extension.json deben coincidir: $source"
    }
    if (-not (Select-String -LiteralPath (Join-Path $source 'CHANGELOG.md') -Pattern "^## $([regex]::Escape($version))(?:\s|$)" -Quiet)) {
        throw "Falta la entrada $version en el changelog de $source"
    }
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    foreach ($name in @('src', 'fixtures', 'README.md', 'README.en.md', 'README.es.md', 'VERSION', 'CHANGELOG.md', 'extension.json')) {
        $item = Join-Path $source $name
        if (Test-Path -LiteralPath $item) { Copy-Item -LiteralPath $item -Destination $destination -Recurse -Force }
    }
    $vendorDirectory = Join-Path $destination 'vendor/xpeed-i18n'
    New-Item -ItemType Directory -Force -Path $vendorDirectory | Out-Null
    foreach ($name in @('src', 'locales', 'Cargo.toml')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "crates/xpeed-i18n/$name") -Destination $vendorDirectory -Recurse -Force
    }
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $vendorDirectory -Force
    $cargoManifest = (Get-Content -LiteralPath (Join-Path $source 'Cargo.toml') -Raw).Replace('../../crates/xpeed-i18n','vendor/xpeed-i18n') + "`n[workspace]`n"
    [IO.File]::WriteAllText((Join-Path $destination 'Cargo.toml'), $cargoManifest, [Text.UTF8Encoding]::new($false))
    cargo generate-lockfile --offline --manifest-path (Join-Path $destination 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo preparar el Cargo.lock del proyecto' }
    New-Item -ItemType Directory -Force -Path (Join-Path $destination 'bin'),(Join-Path $destination 'assets') | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot "target\release\$($entry.Binary).exe") -Destination (Join-Path $destination 'bin') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\img\app-icon.png') -Destination (Join-Path $destination 'assets\icon.png') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $destination -Force
    foreach ($document in @('.editorconfig', '.gitattributes')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot $document) -Destination $destination -Force
    }
    foreach ($notice in @('licenses', 'THIRD_PARTY.txt')) {
        $noticePath = Join-Path $projectRoot "extensions\packages\$($entry.Id)\$notice"
        if (Test-Path -LiteralPath $noticePath) { Copy-Item -LiteralPath $noticePath -Destination $destination -Recurse -Force }
    }
    $manifest = Get-Content -LiteralPath (Join-Path $destination 'extension.json') -Raw | ConvertFrom-Json
    $manifest | Add-Member -NotePropertyName icon -NotePropertyValue 'assets/icon.png' -Force
    [IO.File]::WriteAllText((Join-Path $destination 'extension.json'), ($manifest | ConvertTo-Json -Depth 16), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $destination '.gitignore'), "/target/`n/.tools/`n.env`n.env.*`n!.env.example`n*.log`n*.tmp`n# bin/ is intentionally tracked for direct installation from Git.`n", [Text.UTF8Encoding]::new($false))
    $workflowDirectory = Join-Path $destination '.github/workflows'
    New-Item -ItemType Directory -Force -Path $workflowDirectory | Out-Null
    $workflow = @'
name: Check extension
on: [push, pull_request]

jobs:
  native:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --all --check
      - run: cargo clippy --locked --all-targets -- -D warnings
      - run: cargo test --locked
      - run: cargo build --release --locked
'@
    [IO.File]::WriteAllText((Join-Path $workflowDirectory 'check.yml'), $workflow, [Text.UTF8Encoding]::new($false))
    Write-Output "Proyecto preparado: $destination"
}
Write-Output 'No se han creado repositorios ni publicado archivos. Cada carpeta contiene el proyecto y su ejecutable Windows para subirlos a su propio Git.'
