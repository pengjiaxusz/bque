# tools/generate-release-notes.ps1
#
# Generates structured, bilingual (English & 简体中文) GitHub release notes and SHA-256 checksums.
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

Write-Host "--> Generating bilingual release notes for $Tag (Repo: $Repo)..." -ForegroundColor Cyan

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

# 4. Build Structured Markdown (Bilingual: English + 简体中文)
$sb = [System.Text.StringBuilder]::new()

[void]$sb.AppendLine("# bque $Tag")
[void]$sb.AppendLine()
[void]$sb.AppendLine("[English](#english) | [简体中文](#-简体中文)")
[void]$sb.AppendLine()
[void]$sb.AppendLine("---")
[void]$sb.AppendLine()

# ----------------- English Section -----------------
[void]$sb.AppendLine('<a id="english"></a>')
[void]$sb.AppendLine("## English")
[void]$sb.AppendLine()
[void]$sb.AppendLine("> Universal build queue, CPU governor, and clean-room benchmark protector for Windows.")
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

[void]$sb.AppendLine("---")
[void]$sb.AppendLine()

# ----------------- 简体中文 Section -----------------
[void]$sb.AppendLine('<a id="-简体中文"></a>')
[void]$sb.AppendLine("## 🇨🇳 简体中文")
[void]$sb.AppendLine()
[void]$sb.AppendLine("> 专为 Windows 打造的通用跨工作树编译排队、CPU 调速与纯净跑分守护工具。")
[void]$sb.AppendLine()
[void]$sb.AppendLine("### 🌟 核心亮点")
[void]$sb.AppendLine()
[void]$sb.AppendLine('- **⚡ 无守护进程架构（Daemonless）**：零后台常驻服务与守护进程。底层完全依托 Windows 内核对象（`Named Mutex`、`Job Object`、`Virtual Memory`）。任何异常退出均由内核通过 `AbandonedMutex` 自动安全移交锁，永不遗留死锁。')
[void]$sb.AppendLine('- **🚀 跨工作树编译排队（Cross-Worktree Queue Serialization）**：跨并行 Git Worktree 和多语言工程（CMake/Ninja、Cargo、Go 等）统一协调繁重编译器进程，彻底消除多重编译引发的 CPU 100% 满载与磁盘 IO 锁死。')
[void]$sb.AppendLine('- **🔬 纯净跑分保护（`bque bench`）**：独占执行性能测试时，内核级秒级挂起（`NtSuspendProcess`）所有运行中的后台编译进程树，自动提速至 `High` 优先级并可绑定性能核（P-Core），测试完毕后无缝原位唤醒，绝不丢失编译进度。')
[void]$sb.AppendLine('- **💤 Idle 优先级与动态 CPU 软上限（Soft-Cap）**：默认以 `Idle` 优先级启动任务（子孙进程树自动继承），空闲时满载利用，前台争抢时自适应限流，保障 IDE 与前台界面永远流畅。')
[void]$sb.AppendLine('- **🛡️ Windows 管道卫生与防死锁机制**：针对 Windows 匿名管道 4KB 缓冲区死锁缺陷，内置标准输入抽水机（Stdin Drainer）与专用命名管道通讯，保障大输出量编译永不挂死。')
[void]$sb.AppendLine('- **📦 零依赖一行命令安装器**：提供免管理员提权、自动配置系统 PATH 的 PowerShell 一键安装脚本。')
[void]$sb.AppendLine()
[void]$sb.AppendLine("### 📦 快速安装与上手")
[void]$sb.AppendLine()
[void]$sb.AppendLine("#### 一键安装（推荐）")
[void]$sb.AppendLine('在 PowerShell 中直接运行以下命令（无需管理员提权，自动配置环境变量 `PATH` 并即时生效）：')
[void]$sb.AppendLine('```powershell')
[void]$sb.AppendLine("irm https://github.com/$Repo/releases/latest/download/install.ps1 | iex")
[void]$sb.AppendLine('```')
[void]$sb.AppendLine()
[void]$sb.AppendLine("#### 手动下载")
[void]$sb.AppendLine('在下方下载 `bque-windows-x86_64.zip` 或 `bque.exe`，解压并放置于目录 `$env:LOCALAPPDATA\bque\bin` 即可。')
[void]$sb.AppendLine()
[void]$sb.AppendLine("### 📋 详细更新日志")
[void]$sb.AppendLine()

if ($features.Count -gt 0) {
    [void]$sb.AppendLine("#### 🚀 新特性 (Features)")
    foreach ($item in $features) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}
if ($fixes.Count -gt 0) {
    [void]$sb.AppendLine("#### 🐛 问题修复 (Bug Fixes)")
    foreach ($item in $fixes) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}
if ($perf.Count -gt 0) {
    [void]$sb.AppendLine("#### ⚡ 性能优化 (Performance)")
    foreach ($item in $perf) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}
if ($docs.Count -gt 0) {
    [void]$sb.AppendLine("#### 📚 文档完善 (Documentation)")
    foreach ($item in $docs) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}
if ($ci.Count -gt 0) {
    [void]$sb.AppendLine("#### 🛠️ CI / CD 与自动化 (CI / CD & Automation)")
    foreach ($item in $ci) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}
if ($maintenance.Count -gt 0) {
    [void]$sb.AppendLine("#### 🔧 维护与底层变动 (Maintenance & Chores)")
    foreach ($item in $maintenance) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}
if ($others.Count -gt 0) {
    [void]$sb.AppendLine("#### 🔍 其他变更 (Other Changes)")
    foreach ($item in $others) { [void]$sb.AppendLine($item) }
    [void]$sb.AppendLine()
}

# ----------------- Checksums Table & Links -----------------
if ($hashRecords.Count -gt 0) {
    [void]$sb.AppendLine("---")
    [void]$sb.AppendLine()
    [void]$sb.AppendLine("### 🔒 校验哈希 (SHA-256 Checksums)")
    [void]$sb.AppendLine()
    [void]$sb.AppendLine("| 文件 (File) | 大小 (Size) | SHA-256 Checksum |")
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
Write-Host "--> Bilingual release notes successfully written to: $OutputFile" -ForegroundColor Green
if (Test-Path $ChecksumFile) {
    Write-Host "--> SHA-256 checksums written to: $ChecksumFile" -ForegroundColor Green
}
