# tools/install.ps1
#
# bque One-Liner Quick Installer for Windows
#
# Usage:
#   irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex

$ErrorActionPreference = "Stop"

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$installDir = "$env:LOCALAPPDATA\bque\bin"
$exePath = "$installDir\bque.exe"

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "          Installing bque (Build Queue & Governor)              " -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""

if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
}

$baseUrl = "https://github.com/pengjiaxusz/bque/releases/latest/download"
$zipUrl = "$baseUrl/bque-windows-x86_64.zip"
$directExeUrl = "$baseUrl/bque.exe"
$tempZip = "$installDir\bque-temp.zip"

Write-Host "--> Downloading bque from latest release..." -ForegroundColor Cyan

$downloadSuccess = $false

# 1. 尝试下载 zip 包
try {
    Invoke-RestMethod -Uri $zipUrl -OutFile $tempZip -TimeoutSec 30
    if (Test-Path $tempZip) {
        Write-Host "--> Extracting archive to $installDir..." -ForegroundColor Cyan
        Expand-Archive -Path $tempZip -DestinationPath $installDir -Force
        Remove-Item $tempZip -Force -ErrorAction SilentlyContinue
        $downloadSuccess = $true
    }
} catch {
    Write-Host "--> Zip archive download skipped, trying direct executable..." -ForegroundColor DarkGray
    if (Test-Path $tempZip) {
        Remove-Item $tempZip -Force -ErrorAction SilentlyContinue
    }
}

# 2. 回退：直接下载 bque.exe
if (-not $downloadSuccess -or -not (Test-Path $exePath)) {
    try {
        Invoke-RestMethod -Uri $directExeUrl -OutFile $exePath -TimeoutSec 30
        $downloadSuccess = $true
    } catch {
        Write-Host "❌ Failed to download bque from GitHub Releases." -ForegroundColor Red
        Write-Host "Please check your network connection or download manually from:" -ForegroundColor Yellow
        Write-Host "👉 https://github.com/pengjiaxusz/bque/releases/latest" -ForegroundColor Yellow
        exit 1
    }
}

# 3. 自动追加至当前用户持久化 PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$pathList = if ($userPath) { $userPath -split ';' } else { @() }

if ($pathList -notcontains $installDir) {
    Write-Host "--> Adding $installDir to User PATH..." -ForegroundColor Cyan
    $newPath = if ($userPath) { "$userPath;$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
}

# 4. 同时注入当前终端会话，免重启生效
$env:Path = "$installDir;$env:Path"

# 5. 验证安装
if (Test-Path $exePath) {
    Write-Host ""
    Write-Host "✔ bque installed successfully to $installDir!" -ForegroundColor Green
    Write-Host ""
    & "$exePath" --version
    Write-Host ""
    Write-Host "Ready to use in the current session and new terminal windows." -ForegroundColor Green
} else {
    Write-Host "❌ Installation failed: bque.exe was not found in $installDir" -ForegroundColor Red
    exit 1
}
