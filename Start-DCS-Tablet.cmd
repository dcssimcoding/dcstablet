@echo off
cd /d "%~dp0"
set "DCS_TABLET_EXE=%~dp0dcs-tablet.exe"
if not exist "%DCS_TABLET_EXE%" set "DCS_TABLET_EXE=%~dp0target\release\dcs-tablet.exe"
if not exist "%DCS_TABLET_EXE%" (
 echo Build DCS Tablet or extract a Windows release first.
 pause
 exit /b 1
)
"%DCS_TABLET_EXE%" launch
pause
