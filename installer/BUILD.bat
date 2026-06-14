@echo off
title RoboLaunch Builder
echo ================================
echo  RoboLaunch Build System v1.0
echo ================================

echo [1/3] Build frontend...
cd /d "%~dp0..\frontend"
call npm run build
if errorlevel 1 ( echo ERREUR frontend && pause && exit /b 1 )

echo [2/3] Build Tauri app...
cd /d "%~dp0..\src-tauri"
call cargo tauri build
if errorlevel 1 ( echo ERREUR Tauri && pause && exit /b 1 )

echo [3/3] Build installeur NSIS...
cd /d "%~dp0"
makensis installer.nsi
if errorlevel 1 ( echo ERREUR NSIS && pause && exit /b 1 )

echo.
echo === BUILD OK → RoboLaunch-Setup-1.0.0.exe ===
pause
