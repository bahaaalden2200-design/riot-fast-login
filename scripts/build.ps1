# Automated Production Build Script for Riot Fast Login
param(
    [switch]$NoPause
)

$ErrorActionPreference = "Stop"

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  Riot Fast Login - Production Release Build" -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $repoRoot

# Ensure Cargo is in PATH if installed in default location
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) {
    $env:PATH = "$cargoBin;$env:PATH"
}

# 1. Compile
Write-Host "`n[1/3] Compiling optimized release binary (LTO + Strip)..." -ForegroundColor Yellow
cargo build --release

# 2. Check / download rcedit
$rcedit = "$env:TEMP\rcedit-x64.exe"
if (-not (Test-Path $rcedit)) {
    Write-Host "[2/3] Downloading portable rcedit for icon embedding..." -ForegroundColor Yellow
    Invoke-WebRequest -Uri "https://github.com/electron/rcedit/releases/download/v2.0.0/rcedit-x64.exe" -OutFile $rcedit
} else {
    Write-Host "[2/3] Using cached rcedit..." -ForegroundColor Green
}

# 3. Embed and package
Write-Host "[3/3] Embedding icon and creating dist package..." -ForegroundColor Yellow
$distDir = Join-Path $PSScriptRoot "..\dist"
if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir | Out-Null
}

$binRelease = Join-Path $PSScriptRoot "..\target\release\riot-fast-login.exe"
$iconPath = Join-Path $PSScriptRoot "..\assets\app.ico"
$distExe = Join-Path $distDir "RiotFastLogin.exe"
$rootExe = Join-Path $PSScriptRoot "..\RiotFastLogin.exe"

& $rcedit $binRelease --set-icon $iconPath
Copy-Item $binRelease $distExe -Force
Copy-Item $binRelease $rootExe -Force

# Also provide legacy alias for backward compatibility
$distLegacy = Join-Path $distDir "RiotAccountSwitcher.exe"
$rootLegacy = Join-Path $PSScriptRoot "..\RiotAccountSwitcher.exe"
Copy-Item $binRelease $distLegacy -Force
Copy-Item $binRelease $rootLegacy -Force

# Generate SHA256 Checksum
$hash = (Get-FileHash -Path $distExe -Algorithm SHA256).Hash
$hashLine = "$hash  RiotFastLogin.exe"
Set-Content -Path (Join-Path $distDir "SHA256SUMS.txt") -Value $hashLine

$sizeKb = [Math]::Round((Get-Item $distExe).Length / 1024, 2)

Write-Host "`n========================================================" -ForegroundColor Green
Write-Host " BUILD SUCCESSFUL!" -ForegroundColor Green
Write-Host " Output: $distExe ($sizeKb KB)" -ForegroundColor Green
Write-Host " SHA256: $hash" -ForegroundColor Gray
Write-Host "========================================================" -ForegroundColor Green

if (-not $NoPause) {
    Read-Host "Press Enter to exit..."
}
