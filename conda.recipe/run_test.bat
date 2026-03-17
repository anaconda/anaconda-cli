@echo on

ana
if errorlevel 1 exit /b 1

set "expected=Hello, world!"
for /f "delims=" %%i in ('ana') do set "actual=%%i"
if not "%actual%"=="%expected%" (
  echo FAIL: Output mismatch
  echo   Expected: %expected%
  echo   Actual: %actual%
  exit /b 1
)
