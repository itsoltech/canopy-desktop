@echo off
if "%CANOPY_HOOK_PORT%"=="" exit /b 0
setlocal enabledelayedexpansion
set "INPUT="
for /f "delims=" %%i in ('more') do set "INPUT=!INPUT!%%i"
curl -s -X POST "http://127.0.0.1:%CANOPY_HOOK_PORT%/status" -H "Content-Type: application/json" -H "X-Canopy-Auth: %CANOPY_HOOK_TOKEN%" -d "!INPUT!" 2>nul >nul
echo.
