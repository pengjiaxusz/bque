<#
.SYNOPSIS
    通用子仓库/子模块未应用提交比对与影响面聚类分析脚本。

.DESCRIPTION
    自动比对当前子仓库/子模块锁定的旧 Commit 与目标最新 Commit（支持同级联调仓库、环境变量或远端分支），
    提取两者之间的 oneline 提交记录，并按破坏性变更(Breaking)、约定式提交类别(feat/fix/perf/refactor)
    以及常见领域子系统（动效、高亮、设计令牌、样式层叠、规范组件等）进行影响面聚类分析。
    支持一键拉取 (-Fetch) 与一键签出同步更新 (-Update)。

.PARAMETER SubrepoPath
    子仓库相对路径。若未显式指定，会自动探测常见的子模块目录（如 submodules/*、vendor/* 或 .gitmodules 中配置项）。

.PARAMETER TargetPath
    目标仓库路径。若未指定，优先探测环境变量 $env:<NAME>_DIR、同级目录 ..\<name>；
    若不存在则走子仓库远程分支。

.PARAMETER TargetRef
    目标分支或引用名。默认优先使用 "origin/main"，回退到 "origin/master" 或本地 "HEAD"。

.PARAMETER Fetch
    比对前是否执行 git fetch 以获取远端最新提交。默认为 $true。

.PARAMETER Update
    展示差异后是否直接将子仓库签出更新至目标最新 Commit。默认为 $false。

.PARAMETER ConfigPath
    可选的 JSON 格式聚类规则配置文件路径，用于扩展或覆盖项目专属的关键词分类。

.EXAMPLE
    pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "submodules/cha-set"
    pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "submodules/cha-set" -Update
#>

[CmdletBinding()]
param(
    [string]$SubrepoPath = "",
    [string]$TargetPath = "",
    [string]$TargetRef = "",
    [switch]$Fetch = $true,
    [switch]$Update = $false,
    [string]$ConfigPath = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# 1. 自动探测或验证子仓库路径
function Resolve-SubrepoPath {
    param([string]$Path)
    if ($Path -and (Test-Path $Path)) {
        return (Resolve-Path $Path).Path
    }

    # 探测 .gitmodules
    if (Test-Path ".gitmodules") {
        $modulePaths = git config --file .gitmodules --get-regexp path | ForEach-Object { ($_ -split ' ')[1] }
        foreach ($mp in $modulePaths) {
            if ($mp -and (Test-Path $mp)) {
                return (Resolve-Path $mp).Path
            }
        }
    }

    # 常见候选目录
    $candidates = @("submodules", "external", "vendor", "deps", "packages")
    foreach ($cand in $candidates) {
        if (Test-Path $cand) {
            $subdirs = Get-ChildItem -Path $cand -Directory
            if ($subdirs.Count -eq 1) {
                return $subdirs[0].FullName
            }
        }
    }

    return $null
}

$resolvedSubrepo = Resolve-SubrepoPath $SubrepoPath
if (-not $resolvedSubrepo -or -not (Test-Path $resolvedSubrepo)) {
    Write-Error "未能定位子仓库路径。请通过 -SubrepoPath 指定子仓库目录，例如: -SubrepoPath 'submodules/my-lib'"
}

$subrepoName = Split-Path $resolvedSubrepo -Leaf
Write-Host "检测到目标子仓库: $subrepoName ($resolvedSubrepo)" -ForegroundColor DarkGray

# 2. 读取当前锁定的老位置 Commit
$oldCommitFull = (git -C $resolvedSubrepo rev-parse HEAD).Trim()
$oldCommitShort = (git -C $resolvedSubrepo rev-parse --short HEAD).Trim()
$oldCommitSubject = (git -C $resolvedSubrepo log -1 --pretty=format:"%s" HEAD).Trim()

# 3. 探测目标 Commit 来源
$targetRepoPath = ""
$targetRefResolved = ""

$envVarName = "$($subrepoName.ToUpper().Replace('-', '_'))_DIR"
$envPath = [Environment]::GetEnvironmentVariable($envVarName)

if ($TargetPath -and (Test-Path $TargetPath)) {
    $targetRepoPath = (Resolve-Path $TargetPath).Path
    $targetRefResolved = if ($TargetRef) { $TargetRef } else { "HEAD" }
}
elseif ($envPath -and (Test-Path $envPath)) {
    $targetRepoPath = (Resolve-Path $envPath).Path
    $targetRefResolved = if ($TargetRef) { $TargetRef } else { "HEAD" }
}
elseif (Test-Path "..\\$subrepoName\\.git") {
    $targetRepoPath = (Resolve-Path "..\\$subrepoName").Path
    $targetRefResolved = if ($TargetRef) { $TargetRef } else { "HEAD" }
}
else {
    $targetRepoPath = $resolvedSubrepo
    if ($TargetRef) {
        $targetRefResolved = $TargetRef
    } else {
        # 探测默认远端主分支
        $hasOriginMain = git -C $resolvedSubrepo branch -r --list "origin/main"
        if ($hasOriginMain) {
            $targetRefResolved = "origin/main"
        } else {
            $targetRefResolved = "origin/master"
        }
    }
}

# 4. 执行 Fetch
if ($Fetch) {
    Write-Host "正在从远端拉取最新提交..." -ForegroundColor DarkGray
    try {
        if (Test-Path "$resolvedSubrepo\.git") {
            try { git -C $resolvedSubrepo fetch origin --quiet } catch {}
        }
        if ($targetRepoPath -ne $resolvedSubrepo -and (Test-Path "$targetRepoPath\.git")) {
            try { git -C $targetRepoPath fetch --quiet } catch {}
        }
    }
    catch {
        Write-Warning "Fetch 失败，将使用本地已有缓存进行比对: $_"
    }
}

$newCommitFull = (git -C $targetRepoPath rev-parse $targetRefResolved).Trim()
$newCommitShort = (git -C $targetRepoPath rev-parse --short $targetRefResolved).Trim()
$newCommitSubject = (git -C $targetRepoPath log -1 --pretty=format:"%s" $targetRefResolved).Trim()

Write-Host ""
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host "             $subrepoName 子仓库未应用提交比对分析               " -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host "【当前老位置】: $oldCommitShort ($oldCommitFull)" -ForegroundColor Yellow
Write-Host "               └─ $oldCommitSubject" -ForegroundColor DarkGray
Write-Host "【目标新位置】: $newCommitShort ($newCommitFull) [来源: $targetRepoPath -> $targetRefResolved]" -ForegroundColor Green
Write-Host "               └─ $newCommitSubject" -ForegroundColor DarkGray
Write-Host "-----------------------------------------------------------------" -ForegroundColor Gray

if ($oldCommitFull -eq $newCommitFull) {
    Write-Host "🎉 当前项目已对齐 $subrepoName 的目标提交，无须升级！" -ForegroundColor Green
    return
}

# 5. 提取 oneline 差异列表
$commitsRaw = @()
try {
    $commitsRaw = git -C $targetRepoPath log "$oldCommitFull..$newCommitFull" --oneline --no-merges
}
catch {
    try {
        $commitsRaw = git -C $resolvedSubrepo log "$oldCommitFull..$newCommitFull" --oneline --no-merges
    }
    catch {
        Write-Error "比对提交失败: 无法在仓库中解析提交区间 $oldCommitShort..$newCommitShort"
    }
}

$commitCount = $commitsRaw.Count
Write-Host "发现未应用的提交记录: $commitCount 条" -ForegroundColor Cyan
Write-Host ""

# 6. 多维影响面聚类分析
$breakingCommits = @()
$featCommits = @()
$fixCommits = @()
$perfCommits = @()
$refactorCommits = @()
$motionCommits = @()
$tokenCommits = @()
$styleCommits = @()
$componentCommits = @()
$otherCommits = @()

foreach ($line in $commitsRaw) {
    if (-not $line) { continue }
    $trimmed = $line.Trim()

    # 1. 优先识别破坏性变更 (Breaking Changes)
    if ($trimmed -match "(^[a-f0-9]+\s+[a-z0-9\-]+!\:|BREAKING CHANGE)") {
        $breakingCommits += $trimmed
    }

    # 2. 领域关键词聚类
    if ($trimmed -match "(motion|easing|animate|transition)") {
        $motionCommits += $trimmed
    }
    elseif ($trimmed -match "(token|theme|palette|color)") {
        $tokenCommits += $trimmed
    }
    elseif ($trimmed -match "(styles|cascade-layer|layer|subpath|css)") {
        $styleCommits += $trimmed
    }
    elseif ($trimmed -match "(component|widget|control)") {
        $componentCommits += $trimmed
    }

    # 3. 约定式提交类别分类
    if ($trimmed -match "^[a-f0-9]+\s+feat(\([^\)]+\))?\:") {
        $featCommits += $trimmed
    }
    elseif ($trimmed -match "^[a-f0-9]+\s+fix(\([^\)]+\))?\:") {
        $fixCommits += $trimmed
    }
    elseif ($trimmed -match "^[a-f0-9]+\s+perf(\([^\)]+\))?\:") {
        $perfCommits += $trimmed
    }
    elseif ($trimmed -match "^[a-f0-9]+\s+refactor(\([^\)]+\))?\:") {
        $refactorCommits += $trimmed
    }
    elseif ($trimmed -notmatch "(motion|token|styles|component|feat|fix|perf|refactor)") {
        $otherCommits += $trimmed
    }
}

Write-Host "【涉及系统与影响面聚类】" -ForegroundColor Yellow

if ($breakingCommits.Count -gt 0) {
    Write-Host "  💥 破坏性变更 (Breaking Changes) [$($breakingCommits.Count) 项] - 需重点核验:" -ForegroundColor Red
    $breakingCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($motionCommits.Count -gt 0) {
    Write-Host "  🎬 动效与过渡系统 (Motion) [$($motionCommits.Count) 项]:" -ForegroundColor Magenta
    $motionCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($tokenCommits.Count -gt 0) {
    Write-Host "  🎨 设计令牌与色彩系统 (Tokens & Themes) [$($tokenCommits.Count) 项]:" -ForegroundColor Yellow
    $tokenCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($styleCommits.Count -gt 0) {
    Write-Host "  📐 样式层叠与打包规范 (Styles & Layout) [$($styleCommits.Count) 项]:" -ForegroundColor Green
    $styleCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($componentCommits.Count -gt 0) {
    Write-Host "  🧩 规范控件与组件 API 变更 (Components) [$($componentCommits.Count) 项]:" -ForegroundColor Cyan
    $componentCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($featCommits.Count -gt 0 -and $componentCommits.Count -eq 0) {
    Write-Host "  ✨ 新功能与能力扩展 (Features) [$($featCommits.Count) 项]:" -ForegroundColor Cyan
    $featCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($fixCommits.Count -gt 0) {
    Write-Host "  🐛 缺陷修复与稳定性 (Bug Fixes) [$($fixCommits.Count) 项]:" -ForegroundColor DarkYellow
    $fixCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($perfCommits.Count -gt 0) {
    Write-Host "  ⚡ 性能优化 (Performance) [$($perfCommits.Count) 项]:" -ForegroundColor Green
    $perfCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

if ($otherCommits.Count -gt 0) {
    Write-Host "  🛠️ 工程基建、规范与测试 (Governance & Chore) [$($otherCommits.Count) 项]:" -ForegroundColor DarkGray
    $otherCommits | ForEach-Object { Write-Host "     - $_" }
    Write-Host ""
}

Write-Host "-----------------------------------------------------------------" -ForegroundColor Gray
Write-Host "【完整未应用提交列表 (git log oneline)】:" -ForegroundColor Yellow
$commitsRaw | ForEach-Object { Write-Host "  $_" }
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host ""

# 7. 执行签出更新 (-Update)
if ($Update) {
    # 检查工作区干净度
    $subrepoStatus = (git -C $resolvedSubrepo status --short)
    if ($subrepoStatus) {
        Write-Warning "子仓库工作区有未提交的改动，放弃签出更新。请先清理或提交子仓库内的变更。"
        return
    }

    Write-Host "正在将子仓库 $subrepoName 签出更新至目标 Commit: $newCommitShort ..." -ForegroundColor Yellow
    git -C $resolvedSubrepo checkout $newCommitFull
    $updatedCommit = (git -C $resolvedSubrepo rev-parse --short HEAD).Trim()
    Write-Host "✅ 子仓库已更新到: $updatedCommit" -ForegroundColor Green
    Write-Host ""
    Write-Host "下一步建议：" -ForegroundColor Cyan
    Write-Host "  1. 执行宿主项目对应的编译与类型检查（如 cargo check, pnpm build, just build 等）"
    Write-Host "  2. 按照系统聚类提示适配受影响的业务组件与设计令牌"
    Write-Host "  3. 运行自动化测试与界面门禁进行回归核验"
    Write-Host "  4. 提交时撰写规范提交信息，例如: chore(deps): 更新 $subrepoName 至 $newCommitShort 并完成适配"
}
else {
    Write-Host "💡 提示：若需一键将子仓库签出更新至上述最新提交，可运行：" -ForegroundColor DarkYellow
    Write-Host "  pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath '$SubrepoPath' -Update" -ForegroundColor White
}