@echo on

ana
if errorlevel 1 exit /b 1

for /f "delims=" %%i in ('ana --version') do set "actual=%%i"
if not "%actual%"=="%PKG_VERSION%" (
  echo FAIL: Version mismatch
  echo   Expected: %PKG_VERSION%
  echo   Actual:   %actual%
  exit /b 1
)
