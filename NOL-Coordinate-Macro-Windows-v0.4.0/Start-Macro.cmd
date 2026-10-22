@echo off
setlocal
cd /d "%~dp0"
if not exist "runtime\AutoHotkeyU64.exe" (
  echo Missing official AutoHotkey runtime. Extract the complete ZIP first.
  pause
  exit /b 1
)
"runtime\AutoHotkeyU64.exe" /ErrorStdOut "EnhancedMacro.ahk"
if errorlevel 1 pause
