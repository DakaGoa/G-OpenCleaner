@echo off
setlocal
cd /d "%~dp0"

echo [1/4] Building G-OpenCleaner (release)...
cargo build --release --manifest-path native\Cargo.toml -p openclean
if errorlevel 1 goto :fail

if not exist "native\openclean-setup\assets" mkdir "native\openclean-setup\assets"
echo [2/4] Staging the application payload...
copy /y "native\target\release\openclean.exe" "native\openclean-setup\assets\openclean.exe" >nul
if errorlevel 1 goto :fail

echo [3/4] Building the installer...
cargo build --release --manifest-path native\Cargo.toml -p openclean-setup
if errorlevel 1 goto :fail

if not exist "dist" mkdir "dist"
copy /y "native\target\release\OpenCleanSetup.exe" "dist\G-OpenCleanerSetup.exe" >nul
if errorlevel 1 goto :fail

echo [4/4] Done.
echo.
echo   Installer : dist\G-OpenCleanerSetup.exe
echo   Standalone: native\target\release\openclean.exe
echo.
exit /b 0

:fail
echo.
echo Build failed.
exit /b 1
