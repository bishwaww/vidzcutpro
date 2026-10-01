@echo off
rem Double-click to build (the first time takes a few minutes) and start vidzcut.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\run.ps1" %*
if errorlevel 1 pause
