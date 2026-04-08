REM Disable telemetry and suppress tracing output so otel ERROR messages
REM don't pollute stdout and break version/output assertions.
set ANA_ENABLE_TELEMETRY=false
set RUST_LOG=off

set "BINFILE=%RECIPE_DIR%\..\target\release\ana.exe"
echo Binary path: %BINFILE%
if not exist "%BINFILE%" (
  echo FAIL: Release binary not found
  exit /b 1
)

for /f "delims=" %%i in ('%BINFILE% --version 2^>nul') do set "actual=%%i"
echo Version: %actual%
if not "%actual%"=="%PKG_VERSION%" (
  echo FAIL: Expected %PKG_VERSION%
  exit /b 1
)

mkdir %PREFIX%\bin
if errorlevel 1 exit /b 1
copy "%BINFILE%" %PREFIX%\bin\ana.exe
if errorlevel 1 exit /b 1
