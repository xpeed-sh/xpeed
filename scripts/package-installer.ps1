param([switch]$SkipBuild, [string]$CompilerPath)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$appVersion = (Get-Content -LiteralPath (Join-Path $projectRoot 'VERSION') -Raw).Trim()
if ($appVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'VERSION debe tener el formato x.y.z.' }
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'package.ps1')
}
$payloadDirectory = Join-Path $projectRoot "dist\portable\Xpeed-$appVersion"
foreach ($requiredFile in @('Xpeed.exe', 'LICENSE', 'THIRD_PARTY.md', 'VERSION', 'extensions.json')) {
    if (-not (Test-Path -LiteralPath (Join-Path $payloadDirectory $requiredFile))) { throw "Falta $requiredFile en la entrega portátil. Ejecuta scripts/package.ps1 primero." }
}
if ((Get-Content -LiteralPath (Join-Path $payloadDirectory 'VERSION') -Raw).Trim() -ne $appVersion) { throw 'La versión del paquete no coincide con VERSION.' }
if (-not $CompilerPath) {
    $candidatePaths = @((Join-Path $projectRoot '.tools\inno-setup\ISCC.exe'))
    if (${env:ProgramFiles(x86)}) { $candidatePaths += Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
    if ($env:ProgramFiles) { $candidatePaths += Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe' }
    $CompilerPath = $candidatePaths | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $CompilerPath) { $CompilerPath = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source }
}
if (-not $CompilerPath -or -not (Test-Path -LiteralPath $CompilerPath)) { throw 'Instala Inno Setup 6 desde https://jrsoftware.org/isdl.php o pasa -CompilerPath con la ruta a ISCC.exe.' }
& $CompilerPath "/DAppVersion=$appVersion" "/DProjectRoot=$projectRoot" (Join-Path $projectRoot 'installer\Xpeed.iss')
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del instalador.' }
Write-Output "Comparte este único archivo: $projectRoot\dist\installer\Xpeed-Setup-$appVersion-x64.exe"
