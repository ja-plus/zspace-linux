@echo off
setlocal
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0zspace-cli.ps1" %*
exit /b %ERRORLEVEL%
