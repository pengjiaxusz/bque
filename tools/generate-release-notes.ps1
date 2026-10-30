# tools/generate-release-notes.ps1
#
# Generates structured, professional GitHub release notes and SHA-256 checksums.
#
# Usage:
#   pwsh -NoProfile -File tools/generate-release-notes.ps1 -Tag v0.1.0 -Artifacts "bque.zip", "bque.exe"

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,

    [Parameter(Mandatory = $false)]
    [string]$OutputFile = "release_notes.md",

    [Parameter(Mandatory = $false)]
    [string]$ChecksumFile = "checksums-sha256.txt",

    [Parameter(Mandatory = $false, ValueFromRemainingArguments = $true)]
    [string[]]$Artifacts = @(),

    [Parameter(Mandatory = $false)]
    [string]$Repo = "pengjiaxusz/bque"
)

$ErrorActionPreference = "Stop"

$OutputFile = $OutputFile.Trim().Trim('"').Trim("'")
$ChecksumFile = $ChecksumFile.Trim().Trim('"').Trim("'")

Write-Host "--> Generating release notes for $Tag (Repo: $Repo)..." -ForegroundColor Cyan

# 1. Determine git commit range
$prevTag = $(try {
    git describe --tags --abbrev=0 "${Tag}^" 2>$null
} catch {
    $null
})

if ($prevTag) {
    Write-Host "--> Detected previous release tag: $prevTag" -ForegroundColor Cyan
    $rawLogs = git log "$prevTag..$Tag" --no-merges --pretty=format:"%h|%s|%an"
} else {
    Write-Host "--> No previous tag detected. Extracting all commits up to $Tag..." -ForegroundColor Cyan
    $rawLogs = git log $Tag --no-merges --pretty=format:"%h|%s|%an" 2>$null
    if (-not $rawLogs) {
        $rawLogs = git log --no-merges --pretty=format:"%h|%s|%an"
    }
}

# 2. Categorize Conventional Commits
$features = @()
$fixes = @()
$perf = @()
$docs = @()
$ci = @()
$maintenance = @()
$others = @()

if ($rawLogs) {
    foreach ($line in ($rawLogs -split "`n")) {
        $trimmed = $line.Trim()
        if (-not $trimmed) { continue }
        $parts = $trimmed.Split('|', 3)
        $hash = $parts[0]
        $subject = $parts[1]

        $commitLink = "https://github.com/$Repo/commit/$hash"
        $formattedItem = "- $subject ([``$hash``]($commitLink))"

        if ($subject -match '^(feat)(\(.*\))?:') {
            $features += $formattedItem
        } elseif ($subject -match '^(fix)(\(.*\))?:') {
            $fixes += $formattedItem
        } elseif ($subject -match '^(perf)(\(.*\))?:') {
            $perf += $formattedItem
        } elseif ($subject -match '^(docs)(\(.*\))?:') {
            $docs += $formattedItem
        } elseif ($subject -match '^(ci)(\(.*\))?:') {
            $ci += $formattedItem
        } elseif ($subject -match '^(chore|refactor|test|style|build)(\(.*\))?:') {
            $maintenance += $formattedItem
        } else {
            $others += $formattedItem
        }
    }
}

# 3. Calculate Checksums & Sizes
$hashRecords = @()
if (Test-Path $ChecksumFile) {
    Remove-Item $ChecksumFile -Force
}

$cleanArtifacts = @()
foreach ($item in $Artifacts) {
    if (-not $item) { continue }
    foreach ($sub in ($item -split ',')) {
        $clean = $sub.Trim().Trim('"').Trim("'")
        if ($clean) {
            $cleanArtifacts += $clean
        }
    }
}

foreach ($artifact in $cleanArtifacts) {
    if (-not (Test-Path $artifact)) {
        Write-Warning "Artifact not found: $artifact"
        continue
    }

    $leafName = Split-Path $artifact -Leaf
    $hash = (Get-FileHash -Path $artifact -Algorithm SHA256).Hash.ToLower()
    Add-Content -Path $ChecksumFile -Value "$hash  $leafName"

    $sizeBytes = (Get-Item $artifact).Length
    if ($sizeBytes -ge 1MB) {
        $sizeStr = "{0:N2} MB" -f ($sizeBytes / 1MB)
    } elseif ($sizeBytes -ge 1KB) {
        $sizeStr = "{0:N0} KB" -f ($sizeBytes / 1KB)
    } else {
        $sizeStr = "$sizeBytes B"
    }

    $hashRecords += [PSCustomObject]@{
        Name = $leafName
        Size = $sizeStr
        Hash = $hash
    }
}

# 4. Build Structured Markdown
$sb = [System.Text.StringBuilder]::new()

[void]$sb.AppendLine("# bque $Tag")
[void]$sb.AppendLine()
[void]$sb.AppendLine("> Universal build queue, CPU governor, and clean-room benchmark protector for Windows.")
[void]$sb.AppendLine()
[void]$sb.AppendLine("---")
[void]$sb.AppendLine()
[void]$sb.AppendLine("### 🌟 Key Highlights")
[void]$sb.AppendLine()
[void]$sb.AppendLine('- **⚡ Daemonless Architecture**: Zero background services or daemon processes. Built entirely on Windows kernel primitives (`Named Mutex`, `Job Objects`, `Virtual Memory`). Lock releases are guaranteed on abnormal process termination via `AbandonedMutex`.')
[void]$sb.AppendLine('- **🚀 Cross-Worktree Queue Serialization**: Coordinates heavy compiler processes across parallel git worktrees and multi-repo setups to avoid 100% CPU lockouts and disk IO saturation.')
[void]$sb.AppendLine('- **🔬 Clean-Room Benchmark Protection (`bque bench`)**: Automatically suspends active background builds in kernel space (`NtSuspendProcess`), elevates benchmark execution to `High` priority, pins to P-cores, and seamlessly resumes builds when finished without losing compilation progress.')
[void]$sb.AppendLine('- **💤 Idle Priority & Dynamic CPU Rate Budgets**: Runs builds under `Idle` priority class with Job Object soft-caps, preserving interactive UI responsiveness in IDEs and terminals.')
[void]$sb.AppendLine('- **🛡️ Windows Pipeline Hygiene**: Implements automatic stdin draining and named pipe communication to permanently eliminate 4KB anonymous pipe buffer deadlocks on Windows.')
[void]$sb.AppendLine('- **📦 Zero-Dependency One-Liner Installer**: Quick PowerShell installer requiring no admin privileges.')
[void]$sb.AppendLine()
[void]$sb.AppendLine("---")
[void]$sb.AppendLine()
[void]$sb.AppendLine("### 📦 Quick Start & Installation")
[void]$sb.AppendLine()
[void]$sb.AppendLine("#### One-Liner Quick Install (Recommended)")
[void]$sb.AppendLine('Run in PowerShell (no administrator privileges needed, automatically adds to `PATH`):')
[void]$sb.AppendLine('```powershell')
[void]$sb.AppendLine("irm https://github.com/$Repo/releases/latest/download/install.ps1 | iex")
[void]$sb.AppendLine('```')
[void]$sb.AppendLine()
[void]$sb.AppendLine("#### Manual Download")
[void]$sb.AppendLine('Download `bque-windows-x86_64.zip` or `bque.exe` below and extract it to `$env:LOCALAPPDATA\bque\bin`.')
[void]$sb.AppendLine()
[void]$sb.AppendLine("---")
[void]$sb.AppendLine()
[void]$sb.AppendLine("### 📋 Detailed Changelog")
[void]$sb.AppendLine()

if ($features.Count -gt 0) {
    [void]$sb.AppendLine("#### 🚀 Features")
    foreach ($item in $features) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($fixes.Count -gt 0) {
    [void]$sb.AppendLine("#### 🐛 Bug Fixes")
    foreach ($item in $fixes) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($perf.Count -gt 0) {
    [void]$sb.AppendLine("#### ⚡ Performance")
    foreach ($item in $perf) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($docs.Count -gt 0) {
    [void]$sb.AppendLine("#### 📚 Documentation")
    foreach ($item in $docs) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($ci.Count -gt 0) {
    [void]$sb.AppendLine("#### 🛠️ CI / CD & Automation")
    foreach ($item in $ci) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($maintenance.Count -gt 0) {
    [void]$sb.AppendLine("#### 🔧 Maintenance & Chores")
    foreach ($item in $maintenance) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($others.Count -gt 0) {
    [void]$sb.AppendLine("#### 🔍 Other Changes")
    foreach ($item in $others) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

if ($hashRecords.Count -gt 0) {
    [void]$sb.AppendLine("---")
    [void]$sb.AppendLine()
    [void]$sb.AppendLine("### 🔒 Verify Checksums (SHA-256)")
    [void]$sb.AppendLine()
    [void]$sb.AppendLine("| File | Size | SHA-256 Checksum |")
    [void]$sb.AppendLine("| :--- | :--- | :--- |")
    foreach ($rec in $hashRecords) {
        [void]$sb.AppendLine("| ``$($rec.Name)`` | $($rec.Size) | ``$($rec.Hash)`` |")
    }
    [void]$sb.AppendLine()
}

[void]$sb.AppendLine("---")
[void]$sb.AppendLine()
if ($prevTag) {
    [void]$sb.AppendLine("**Full Changelog**: https://github.com/$Repo/compare/$prevTag...$Tag")
} else {
    [void]$sb.AppendLine("**Full Changelog**: https://github.com/$Repo/commits/$Tag")
}

$sb.ToString() | Set-Content -Path $OutputFile -Encoding utf8
Write-Host "--> Release notes successfully written to: $OutputFile" -ForegroundColor Green
if (Test-Path $ChecksumFile) {
    Write-Host "--> SHA-256 checksums written to: $ChecksumFile" -ForegroundColor Green
}
