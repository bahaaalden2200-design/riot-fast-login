@echo off
setlocal enabledelayedexpansion

echo ========================================================
echo   Riot Account Switcher - Production Release Build
echo ========================================================

cd /d "%~dp0\.."

:: 1. Check Cargo
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
)
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] Cargo / Rust not found in PATH!
    pause
    exit /b 1
)

:: 2. Build Release with LTO
echo [1/3] Compiling optimized release binary...
cargo build --release
if %errorlevel% neq 0 (
    echo [ERROR] Cargo build failed!
    pause
    exit /b 1
)

:: 3. Prepare rcedit
set RCEDIT=%TEMP%\rcedit-x64.exe
if not exist "%RCEDIT%" (
    echo [2/3] Downloading portable rcedit for icon embedding...
    powershell -Command "Invoke-WebRequest -Uri 'https://github.com/electron/rcedit/releases/download/v2.0.0/rcedit-x64.exe' -OutFile '%RCEDIT%'"
)

:: 4. Package Release
echo [3/3] Embedding icon and packaging to dist/...
if not exist "dist" mkdir dist
"%RCEDIT%" "target\release\riot-account-switcher.exe" --set-icon "assets\app.ico"
copy /Y "target\release\riot-account-switcher.exe" "dist\RiotAccountSwitcher.exe" >nul
copy /Y "target\release\riot-account-switcher.exe" "RiotAccountSwitcher.exe" >nul

echo.
echo ========================================================
echo  BUILD SUCCESSFUL!
echo  Output: dist\RiotAccountSwitcher.exe
echo ========================================================
pause
