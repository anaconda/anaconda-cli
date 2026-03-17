@echo on

cargo build --release
if errorlevel 1 exit /b 1

mkdir %PREFIX%\bin
copy target\release\ana.exe %PREFIX%\bin\
if errorlevel 1 exit /b 1
