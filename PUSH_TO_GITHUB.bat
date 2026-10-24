@echo off
title Push Riot Fast Login to GitHub
cd /d "%~dp0"
echo ========================================================
echo   Uploading Riot Fast Login to GitHub...
echo ========================================================
echo.
"%LOCALAPPDATA%\Programs\MinGit\cmd\git.exe" push -u origin main
echo.
if %ERRORLEVEL% EQU 0 (
    echo ========================================================
    echo   SUCCESS! The project is now live on GitHub!
    echo ========================================================
) else (
    echo.
    echo Something went wrong. If a browser window opened, please authorize Git.
)
echo.
pause
