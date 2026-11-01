@echo off
rem Double-click to make the mainnet keys in C:\TiberMainnet\keys (never replaces an existing one).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0make-mainnet-keys.ps1"
echo.
pause
