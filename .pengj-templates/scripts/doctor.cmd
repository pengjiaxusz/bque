@echo off
setlocal EnableExtensions
chcp 65001 >nul
rem Managed by pengj-templates (common layer).
rem Non-interactive bootstrap wrapper for repository environment checks.
rem Never downloads/installs tools automatically and never prompts for y/n input.

set "PENGJ_BIN="
if defined LOCALAPPDATA set "PENGJ_BIN=%LOCALAPPDATA%\pengj-templates\bin"
if not defined PENGJ_BIN goto :path_done
if not exist "%PENGJ_BIN%" goto :path_done
echo ";%PATH%;" | findstr /I /C:";%PENGJ_BIN%;" >nul 2>&1
if errorlevel 1 set "PATH=%PATH%;%PENGJ_BIN%"
:path_done

set "HAS_JUST=0"
where just >nul 2>&1 && set "HAS_JUST=1"
set "HAS_CLI=0"
where pengj-templates-cli >nul 2>&1 && set "HAS_CLI=1"
set "HAS_PWSH=0"
where pwsh >nul 2>&1 && set "HAS_PWSH=1"
set "HAS_JUSTFILE=0"
if exist "justfile" set "HAS_JUSTFILE=1"

if "%HAS_JUST%"=="1" if "%HAS_CLI%"=="1" if "%HAS_PWSH%"=="1" if "%HAS_JUSTFILE%"=="1" goto :run_just
if "%HAS_CLI%"=="1" goto :run_cli
goto :missing_tools

:run_just
just doctor %*
exit /b %ERRORLEVEL%

:run_cli
pengj-templates-cli doctor --dir . %*
exit /b %ERRORLEVEL%

:missing_tools
set "DISPLAY_BIN=%%LOCALAPPDATA%%\pengj-templates\bin"
if defined PENGJ_BIN set "DISPLAY_BIN=%PENGJ_BIN%"
>&2 echo [doctor] Missing required local tool(s). Nothing was downloaded or modified.
>&2 echo.
>&2 echo   - pengj-templates-cli (~5-10 MB)
>&2 echo     Default install path: %DISPLAY_BIN%
>&2 echo     Install (PowerShell):
>&2 echo       irm https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.ps1 ^| iex

if "%HAS_JUST%"=="1" goto :check_pwsh
>&2 echo.
>&2 echo   - just (~3-5 MB, optional if calling the CLI directly)
>&2 echo     Typical install path: WinGet / Scoop shims or %%USERPROFILE%%\.cargo\bin
>&2 echo     Install: winget install Casey.Just / scoop install just / https://github.com/casey/just#installation

:check_pwsh
if "%HAS_PWSH%"=="1" goto :footer
>&2 echo.
>&2 echo   - pwsh (PowerShell 7, ~100 MB; required by just recipes on Windows, optional if calling the CLI directly)
>&2 echo     Install: winget install Microsoft.PowerShell

:footer
>&2 echo.
>&2 echo After installing, re-run:
>&2 echo   .pengj-templates\scripts\doctor.cmd
>&2 echo   # or: just doctor (pass --fix explicitly to fix local hooks/links)
exit /b 1