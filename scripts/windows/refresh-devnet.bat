@echo off
rem Double-click to bring devnet up to the loyalty-burn version. Output is also saved to devnet-log.txt.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0refresh-devnet.ps1"
echo.
pause
