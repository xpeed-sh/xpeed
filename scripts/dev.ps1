param([switch]$Release, [switch]$Test)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
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
if ($Test) { cargo test } elseif ($Release) { cargo run --release } else { cargo run }
exit $LASTEXITCODE
