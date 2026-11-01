@echo off
rem Double-click to upgrade the five devnet programs to the reviewed version. Output is also saved to devnet-log.txt.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0upgrade-devnet.ps1"
echo.
pause
