@echo off
if "%CANOPY_HOOK_PORT%"=="" exit /b 0
setlocal enabledelayedexpansion
set "INPUT="
for /f "delims=" %%i in ('more') do set "INPUT=!INPUT!%%i"
for /f "delims=" %%r in ('curl -s -X POST "http://127.0.0.1:%CANOPY_HOOK_PORT%/hook" -H "Content-Type: application/json" -d "!INPUT!" 2^>nul') do echo %%r
