@echo off
chcp 65001 >nul
echo Запуск установки Dungeonborne Custom Server Mod...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
pause