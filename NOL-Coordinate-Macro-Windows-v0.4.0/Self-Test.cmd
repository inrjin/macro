@echo off
setlocal
cd /d "%~dp0"
"runtime\AutoHotkeyU64.exe" /ErrorStdOut "EnhancedMacro.ahk" --self-test
set "TestResult=%ERRORLEVEL%"
echo Macro self-test exit code: %TestResult%
pause
exit /b %TestResult%
