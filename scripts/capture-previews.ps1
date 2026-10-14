param([string]$Executable)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (-not $Executable) { $Executable = Join-Path $projectRoot 'target/release/xpeed.exe' }
$destination = Join-Path $projectRoot 'assets/previews'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$shots = @(
    @{Name='workspace-dark'; Args=@('--demo-workspace','--demo-about')},
    @{Name='workspace-light'; Args=@('--demo-workspace','--demo-about','--light')},
    @{Name='theme-carbon'; Args=@('--demo-workspace','--theme','0')},
    @{Name='theme-midnight'; Args=@('--demo-workspace','--theme','1')},
    @{Name='theme-forest'; Args=@('--demo-workspace','--theme','2')},
    @{Name='theme-porcelain'; Args=@('--demo-workspace','--light','--theme','0')},
    @{Name='theme-paper'; Args=@('--demo-workspace','--light','--theme','1')},
    @{Name='theme-ice'; Args=@('--demo-workspace','--light','--theme','2')},
    @{Name='extensions'; Args=@('--demo-extension-manager','--light')},
    @{Name='extension-details'; Args=@('--demo-extension-manager','--demo-installed-details','xpeed.codex')}
)
foreach ($shot in $shots) {
    $path = Join-Path $destination "$($shot.Name).png"
    $arguments = @('--capture', ('"' + $path + '"'), '--language', 'es') + $shot.Args
    $process = Start-Process -FilePath $Executable -ArgumentList $arguments -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(20000)) { throw "Capture timed out: $($shot.Name)" }
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $path)) { throw "Capture failed: $($shot.Name)" }
    Write-Output "Captured: $($shot.Name)"
}
# Keep existing documentation links current.
Copy-Item -LiteralPath (Join-Path $destination 'workspace-dark.png') -Destination (Join-Path $projectRoot 'assets/preview.png') -Force
Copy-Item -LiteralPath (Join-Path $destination 'workspace-light.png') -Destination (Join-Path $projectRoot 'assets/preview-light.png') -Force
Copy-Item -LiteralPath (Join-Path $destination 'workspace-dark.png') -Destination (Join-Path $projectRoot 'assets/preview-terminal.png') -Force
