# tools/install.ps1
#
# bque One-Liner Quick Installer & Updater for Windows
#
# Usage:
#   irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex

$ErrorActionPreference = "Stop"

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls11 -bor [Net.SecurityProtocolType]::Tls

$defaultInstallDir = "$env:LOCALAPPDATA\bque\bin"
$defaultExe = "$defaultInstallDir\bque.exe"

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "          bque (Build Queue & Governor) Installer / Updater     " -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Detect existing bque installations on PATH
$existingCommands = @(Get-Command bque -All -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)
$targetPaths = [System.Collections.Generic.List[string]]::new()

$oldVersion = ""
if ($existingCommands.Count -gt 0) {
    try {
        $oldVerOutput = & $existingCommands[0] --version 2>$null
        if ($oldVerOutput -match '(\d+\.\d+\.\d+)') {
            $oldVersion = $matches[1]
        }
    } catch {}

    foreach ($cmd in $existingCommands) {
        if (-not $targetPaths.Contains($cmd)) {
            $targetPaths.Add($cmd)
        }
    }
}

if (-not $targetPaths.Contains($defaultExe)) {
    $targetPaths.Add($defaultExe)
}

if (-not (Test-Path $defaultInstallDir)) {
    New-Item -ItemType Directory -Force -Path $defaultInstallDir | Out-Null
}

# 2. Download helper with curl and proxy fallback
function Download-FileWithFallback {
    param (
        [string]$Url,
        [string]$Destination
    )

    # 2.1 Prefer curl.exe (inherits proxy settings, TLS 1.3, fast)
    $curlPath = "C:\Windows\System32\curl.exe"
    if (Test-Path $curlPath) {
        & $curlPath -fLs --connect-timeout 15 --max-time 60 -o $Destination $Url 2>$null
        if ((Test-Path $Destination) -and ((Get-Item $Destination).Length -gt 100000)) {
            return $true
        }
    }

    # 2.2 Fallback: HttpWebRequest with default system web proxy
    try {
        $proxy = [System.Net.WebRequest]::GetSystemWebProxy()
        $proxy.Credentials = [System.Net.CredentialCache]::DefaultCredentials
        
        $req = [System.Net.HttpWebRequest]::Create($Url)
        $req.Proxy = $proxy
        $req.Timeout = 30000
        $req.UserAgent = "bque-installer"
        $resp = $req.GetResponse()
        $stream = $resp.GetResponseStream()
        $fs = [System.IO.File]::Create($Destination)
        $stream.CopyTo($fs)
        $fs.Close()
        $stream.Close()
        $resp.Close()

        if ((Test-Path $Destination) -and ((Get-Item $Destination).Length -gt 100000)) {
            return $true
        }
    } catch {}

    # 2.3 Fallback: Invoke-RestMethod
    try {
        Invoke-RestMethod -Uri $Url -OutFile $Destination -TimeoutSec 30
        if ((Test-Path $Destination) -and ((Get-Item $Destination).Length -gt 100000)) {
            return $true
        }
    } catch {}

    return $false
}

$baseUrl = "https://github.com/pengjiaxusz/bque/releases/latest/download"
$directExeUrl = "$baseUrl/bque.exe"
$tempExe = "$env:TEMP\bque-download-$([System.Guid]::NewGuid().ToString('N')).exe"

Write-Host "--> Downloading latest bque from GitHub Releases..." -ForegroundColor Cyan

$downloadSuccess = Download-FileWithFallback -Url $directExeUrl -Destination $tempExe

if (-not $downloadSuccess) {
    Write-Host "--> Direct executable download failed, trying zip archive..." -ForegroundColor DarkGray
    $zipUrl = "$baseUrl/bque-windows-x86_64.zip"
    $tempZip = "$env:TEMP\bque-temp-$([System.Guid]::NewGuid().ToString('N')).zip"
    $tempExtract = "$env:TEMP\bque-extract-$([System.Guid]::NewGuid().ToString('N'))"
    try {
        if (Download-FileWithFallback -Url $zipUrl -Destination $tempZip) {
            New-Item -ItemType Directory -Force -Path $tempExtract | Out-Null
            Expand-Archive -Path $tempZip -DestinationPath $tempExtract -Force
            $extractedExe = Get-ChildItem -Path $tempExtract -Filter "bque.exe" -Recurse | Select-Object -First 1
            if ($extractedExe) {
                Copy-Item -Path $extractedExe.FullName -Destination $tempExe -Force
                $downloadSuccess = $true
            }
        }
    } catch {
        Write-Host "Failed to extract zip archive." -ForegroundColor DarkGray
    } finally {
        Remove-Item $tempZip -Force -ErrorAction SilentlyContinue
        Remove-Item $tempExtract -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if (-not $downloadSuccess -or -not (Test-Path $tempExe)) {
    Write-Host "Failed to acquire bque.exe from GitHub Releases." -ForegroundColor Red
    Write-Host "Please check your network connection or download manually from:" -ForegroundColor Yellow
    Write-Host "https://github.com/pengjiaxusz/bque/releases/latest" -ForegroundColor Yellow
    exit 1
}

# 3. Deploy to target locations with safe hot-swap (rename-then-replace)
$installedPaths = @()
foreach ($target in $targetPaths) {
    $dir = Split-Path -Parent $target
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }

    $oldBackup = "$target.$([System.Guid]::NewGuid().ToString('N')).old"
    if (Test-Path $target) {
        try {
            Move-Item -Path $target -Destination $oldBackup -Force -ErrorAction Stop
        } catch {
            Write-Host "Warning: Could not replace $target (file locked): $_" -ForegroundColor Yellow
            continue
        }
    }

    Copy-Item -Path $tempExe -Destination $target -Force
    Remove-Item $oldBackup -Force -ErrorAction SilentlyContinue
    $installedPaths += $target

    # Clean up older .old files in target directory
    Get-ChildItem -Path $dir -Filter "bque.*.old" -ErrorAction SilentlyContinue | ForEach-Object {
        Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue
    }
}

Remove-Item $tempExe -Force -ErrorAction SilentlyContinue

# 4. Ensure default install dir is in User PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$pathList = if ($userPath) { $userPath -split ';' } else { @() }

if ($pathList -notcontains $defaultInstallDir) {
    Write-Host "--> Adding $defaultInstallDir to User PATH..." -ForegroundColor Cyan
    $newPath = if ($userPath) { "$userPath;$defaultInstallDir" } else { $defaultInstallDir }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
}

# 5. Also ensure it is in the current process PATH
$env:Path = "$defaultInstallDir;$env:Path"

# 6. Verify installed binary
$primaryExe = if ($installedPaths.Count -gt 0) { $installedPaths[0] } else { $defaultExe }
if (Test-Path $primaryExe) {
    Write-Host ""
    $newVerOutput = & "$primaryExe" --version 2>$null
    if ($oldVersion -and $oldVersion -ne "") {
        Write-Host "✔ bque updated successfully: v$oldVersion -> $newVerOutput" -ForegroundColor Green
    } else {
        Write-Host "✔ bque installed successfully!" -ForegroundColor Green
        Write-Host "Version: $newVerOutput" -ForegroundColor Green
    }
    Write-Host "Installed / updated location(s):" -ForegroundColor Cyan
    foreach ($p in $installedPaths) {
        Write-Host "  • $p" -ForegroundColor Gray
    }
    Write-Host ""
    Write-Host "Ready to use in the current session and new terminal windows." -ForegroundColor Green
} else {
    Write-Host "Installation failed: bque.exe could not be verified." -ForegroundColor Red
    exit 1
}
