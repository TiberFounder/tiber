@echo off
rem Double-click to run the Romulus and Remus devnet launch. Output is also saved to devnet-log.txt.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-devnet.ps1"
echo.
pause
