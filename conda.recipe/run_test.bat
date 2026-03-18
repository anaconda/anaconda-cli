@echo on

ana
if errorlevel 1 exit /b 1

for /f "delims=" %%i in ('ana') do set "actual=%%i"
echo %actual% | findstr /C:"Hello, world! (v" >nul
if errorlevel 1 (
  echo FAIL: Output mismatch
  echo   Expected: Hello, world! ^(v*^)
  echo   Actual: %actual%
  exit /b 1
)
