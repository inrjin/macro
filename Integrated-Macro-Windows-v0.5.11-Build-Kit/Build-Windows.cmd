@echo off
setlocal EnableExtensions DisableDelayedExpansion
if /i "%VSCMD_ARG_TGT_ARCH%"=="x64" goto build
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" goto missing_tools
set "VS_INSTALL="
set "VS_QUERY_FILE=%TEMP%\integrated-macro-vswhere-%RANDOM%-%RANDOM%.txt"
if exist "%VS_QUERY_FILE%" goto environment_failed
"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath > "%VS_QUERY_FILE%"
set "VS_QUERY_EXIT=%ERRORLEVEL%"
if exist "%VS_QUERY_FILE%" for /f "usebackq delims=" %%I in ("%VS_QUERY_FILE%") do set "VS_INSTALL=%%I"
if exist "%VS_QUERY_FILE%" del /q "%VS_QUERY_FILE%"
if not "%VS_QUERY_EXIT%"=="0" goto environment_failed
if not defined VS_INSTALL goto missing_tools
if not exist "%VS_INSTALL%\Common7\Tools\VsDevCmd.bat" goto missing_tools
call "%VS_INSTALL%\Common7\Tools\VsDevCmd.bat" -no_logo -arch=x64 -host_arch=x64
if errorlevel 1 goto environment_failed
if /i not "%VSCMD_ARG_TGT_ARCH%"=="x64" goto environment_failed
:build
rem Let Windows PowerShell rebuild its module paths instead of inheriting PowerShell 7 paths.
set "PSModulePath="
powershell.exe -NoLogo -NoProfile -File "%~dp0build\Build-Windows.ps1" %*
set "BUILD_EXIT=%ERRORLEVEL%"
if not "%BUILD_EXIT%"=="0" echo Build failed. No successful Windows package is claimed. See the output log if one was created.
exit /b %BUILD_EXIT%
:missing_tools
echo Missing Visual Studio C++ x64 Build Tools. Install C++ desktop tools and the Windows SDK, then retry.
exit /b 1
:environment_failed
echo Could not initialize the x64 Visual Studio build environment. No build was started.
exit /b 1
