@echo off
if "%CANOPY_HOOK_PORT%"=="" exit /b 0
curl -s -X POST "http://127.0.0.1:%CANOPY_HOOK_PORT%%CANOPY_HOOK_PATH%/status" ^
  -H "Content-Type: application/json" ^
  -H "X-Canopy-Auth: %CANOPY_HOOK_TOKEN%" ^
  --data-binary @- 2>nul >nul
echo.
