@echo off
setlocal
title Notchix Setup
cd /d "%~dp0"
echo.
echo  NOTCHIX  -  INSTALL / UPDATE
echo  ==================================
echo.
echo  This builds Notchix on your PC and installs a Windows app package.
echo  Your files and settings are not removed.
echo  Please exit any running Notchix from the tray before continuing.
echo.
pause
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0packaging\windows\Install-Notchix.ps1"
set "result=%errorlevel%"
echo.
if not "%result%"=="0" (
    echo  Setup did not complete. Please copy the error text above.
) else (
    echo  Setup finished. Open Notchix from the Windows Start menu.
    echo  If prompted, grant notification access inside Activity.
)
echo.
pause
exit /b %result%
