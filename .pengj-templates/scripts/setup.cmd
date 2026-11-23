@echo off
setlocal EnableExtensions
chcp 65001 >nul
rem Managed by pengj-templates (common layer).
rem Zero-prerequisite thin bootstrap wrapper for repository environment setup.
rem Installs pengj-templates-cli if missing, then hands off to `pengj-templates-cli setup --dir .`.

set "PENGJ_BIN="
if defined LOCALAPPDATA set "PENGJ_BIN=%LOCALAPPDATA%\pengj-templates\bin"
if not defined PENGJ_BIN goto :path_done
echo ";%PATH%;" | findstr /I /C:";%PENGJ_BIN%;" >nul 2>&1
if errorlevel 1 set "PATH=%PATH%;%PENGJ_BIN%"
:path_done

where pengj-templates-cli >nul 2>&1
if not errorlevel 1 goto :run_cli

set "DRY_RUN=0"
for %%A in (%*) do (
  if /I "%%~A"=="--dry-run" set "DRY_RUN=1"
)
if "%DRY_RUN%"=="1" goto :dry_run

set "DISPLAY_BIN=%%LOCALAPPDATA%%\pengj-templates\bin"
if defined PENGJ_BIN set "DISPLAY_BIN=%PENGJ_BIN%"
>&2 echo [setup] Installing pengj-templates-cli to %DISPLAY_BIN%...
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.ps1 | iex"
if errorlevel 1 exit /b %ERRORLEVEL%

if defined GITHUB_PATH if defined PENGJ_BIN (
  >>"%GITHUB_PATH%" echo %PENGJ_BIN%
)

where pengj-templates-cli >nul 2>&1
if errorlevel 1 (
  >&2 echo [setup] Error: pengj-templates-cli was not found in PATH after installation ^(%DISPLAY_BIN%^).
  exit /b 1
)
goto :run_cli

:dry_run
set "DISPLAY_BIN=%%LOCALAPPDATA%%\pengj-templates\bin"
if defined PENGJ_BIN set "DISPLAY_BIN=%PENGJ_BIN%"
echo [setup] [dry-run] pengj-templates-cli is not installed.
echo [setup] [dry-run] Would download and install pengj-templates-cli to %DISPLAY_BIN%, then run: pengj-templates-cli setup --dir .
exit /b 0

:run_cli
pengj-templates-cli setup --dir . %*
exit /b %ERRORLEVEL%