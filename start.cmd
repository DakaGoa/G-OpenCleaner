@echo off
setlocal
cd /d "%~dp0"
where node >nul 2>nul
if errorlevel 1 (
  echo Node.js 18 or newer is required. Get it from https://nodejs.org
  pause
  exit /b 1
)
echo Starting OpenClean...
node server\index.js
pause
