@echo off
cd /d "%~dp0"
if exist "%~dp0dist\Xpeed-0.10.7.exe" (
    start "" "%~dp0dist\Xpeed-0.10.7.exe"
) else if exist "%~dp0dist\Xpeed.exe" (
    start "" "%~dp0dist\Xpeed.exe"
) else (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\dev.ps1" -Release
)
