<#
.SYNOPSIS
    只读全分支拓扑与净贡献巡检工具 (Show Branch Topology)

.DESCRIPTION
    只读探测当前仓库所有本地分支、远端跟踪状态、Worktree 占用情况，以及各分支相对集成分支的净新增提交（git cherry）。
    不执行任何写操作，供 Agent 或开发者快速了解分支状态。

.PARAMETER IntegrationBranch
    集成分支名。若未指定，自动根据当前目录激活分支或多层降级策略智能推导。

.PARAMETER Detailed
    是否展示每个分支的具体净提交标题列表。

.PARAMETER NoFetch
    跳过 git fetch origin。
#>

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$IntegrationBranch = "",

    [switch]$Detailed,
    [switch]$NoFetch
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

function Format-GitArg {
    param([string]$Arg)
    if ($Arg -match '[\s"]') {
        return '"' + ($Arg -replace '"', '\"') + '"'
    }
    return $Arg
}

function Invoke-Git {
    param(
        [string[]]$CommandArgs,
        [string]$WorkingDir = "",
        [int]$TimeoutSeconds = 300
    )
    $pinfo = New-Object System.Diagnostics.ProcessStartInfo
    $pinfo.FileName = "git"
    $pinfo.Arguments = ($CommandArgs | ForEach-Object { Format-GitArg $_ }) -join " "
    $pinfo.RedirectStandardOutput = $true
    $pinfo.RedirectStandardError = $true
    $pinfo.UseShellExecute = $false
    $pinfo.CreateNoWindow = $true
    $pinfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $pinfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    if ($WorkingDir) {
        $pinfo.WorkingDirectory = $WorkingDir
    }

    # 防交互与外部编辑器挂死：禁用交互式凭据提示与外部编辑器弹窗
    $isWin = [System.Environment]::OSVersion.Platform -like "*Win*" -or $IsWindows
    $pinfo.Environment["GIT_TERMINAL_PROMPT"] = "0"
    $pinfo.Environment["GIT_OPTIONAL_LOCKS"] = "0"
    if ($isWin) {
        $pinfo.Environment["GIT_EDITOR"] = "cmd.exe /c exit 0"
        $pinfo.Environment["GIT_SEQUENCE_EDITOR"] = $pinfo.Environment["GIT_EDITOR"]
        $pinfo.Environment["GIT_PAGER"] = "cat"
    } else {
        $pinfo.Environment["GIT_EDITOR"] = "true"
        $pinfo.Environment["GIT_SEQUENCE_EDITOR"] = $pinfo.Environment["GIT_EDITOR"]
        $pinfo.Environment["GIT_PAGER"] = "cat"
    }

    $process = [System.Diagnostics.Process]::Start($pinfo)
    $outTask = $process.StandardOutput.ReadToEndAsync()
    $errTask = $process.StandardError.ReadToEndAsync()

    $timeoutMs = if ($TimeoutSeconds -gt 0) { $TimeoutSeconds * 1000 } else { [System.Threading.Timeout]::Infinite }
    $exited = $process.WaitForExit($timeoutMs)

    if (-not $exited) {
        try {
            $process.Kill($true)
        } catch {
            try { $process.Kill() } catch {}
        }
        $cmdStr = "git " + (($CommandArgs | ForEach-Object { Format-GitArg $_ }) -join " ")
        throw "Git 命令执行超时（超过 $($TimeoutSeconds)s）已被强制终止：$cmdStr"
    }

    $process.WaitForExit()
    [System.Threading.Tasks.Task]::WaitAll(@($outTask, $errTask))

    return [PSCustomObject]@{
        ExitCode = $process.ExitCode
        Output   = $outTask.Result.Trim()
        Error    = $errTask.Result.Trim()
    }
}

function Resolve-IntegrationBranch {
    param(
        [string]$ExplicitBranch,
        [string]$DeclaredBranch,
        [bool]$HasRemote
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitBranch)) {
        return $ExplicitBranch.Trim().Replace("refs/heads/", "")
    }

    $symRef = Invoke-Git @("symbolic-ref", "--short", "-q", "HEAD")
    if ($symRef.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($symRef.Output))) {
        return $symRef.Output.Trim()
    }

    $currentPath = (Get-Location).Path
    try { $currentPath = (Resolve-Path $currentPath).Path } catch {}
    $wtList = Invoke-Git @("worktree", "list", "--porcelain")
    if ($wtList.ExitCode -eq 0) {
        $lines = $wtList.Output -split "`r?`n"
        $wPath = ""
        foreach ($line in $lines) {
            if ($line.StartsWith("worktree ")) {
                $wPath = $line.Substring(9).Trim()
                try { $wPath = (Resolve-Path $wPath).Path } catch {}
            } elseif ($line.StartsWith("branch refs/heads/")) {
                $bName = $line.Substring(18).Trim()
                if ($wPath -eq $currentPath -and (-not [string]::IsNullOrWhiteSpace($bName))) {
                    return $bName
                }
            }
        }
    }

    $pointsAt = Invoke-Git @("branch", "--points-at", "HEAD", "--format=%(refname:short)")
    if ($pointsAt.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($pointsAt.Output))) {
        $candidates = @($pointsAt.Output -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and -not $_.StartsWith("(") })
        if ($candidates.Count -eq 1) {
            return $candidates[0]
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($DeclaredBranch)) {
        return $DeclaredBranch
    }

    if ($HasRemote) {
        $rHead = Invoke-Git @("symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD")
        if ($rHead.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($rHead.Output))) {
            $rh = $rHead.Output.Trim()
            if ($rh.StartsWith("origin/")) { $rh = $rh.Substring(7).Trim() }
            return $rh
        }
        foreach ($b in @("main", "dev", "develop", "master", "trunk")) {
            if ((Invoke-Git @("rev-parse", "--verify", "origin/$b")).ExitCode -eq 0) {
                return $b
            }
        }
    }

    foreach ($b in @("main", "dev", "develop", "master", "trunk")) {
        if ((Invoke-Git @("rev-parse", "--verify", $b)).ExitCode -eq 0) {
            return $b
        }
    }

    $dConf = Invoke-Git @("config", "init.defaultBranch")
    if ($dConf.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($dConf.Output))) {
        return $dConf.Output.Trim()
    }
    return "main"
}

function Get-WorktreeMap {
    $wtList = Invoke-Git @("worktree", "list", "--porcelain")
    $map = @{}
    $allPaths = [System.Collections.Generic.List[string]]::new()
    if ($wtList.ExitCode -eq 0) {
        $lines = $wtList.Output -split "`r?`n"
        $currentPath = ""
        foreach ($line in $lines) {
            if ($line.StartsWith("worktree ")) {
                $currentPath = $line.Substring(9).Trim()
                try { $currentPath = (Resolve-Path $currentPath).Path } catch {}
                $allPaths.Add($currentPath)
            } elseif ($line.StartsWith("branch refs/heads/")) {
                $bName = $line.Substring(18).Trim()
                if ($currentPath -and $bName) {
                    $map[$bName] = $currentPath
                }
            } elseif ([string]::IsNullOrWhiteSpace($line)) {
                $currentPath = ""
            }
        }
    }
    return [PSCustomObject]@{
        BranchToPath = $map
        AllPaths     = $allPaths
    }
}

# 读取 SKILL.md
$skillDocPath = Join-Path $PSScriptRoot "..\SKILL.md"
if (-not (Test-Path $skillDocPath)) {
    $candidate = Join-Path (Get-Location).Path ".agents\skills\git-workflow\SKILL.md"
    if (Test-Path $candidate) {
        $skillDocPath = $candidate
    }
}
$declaredIntegrationBranch = ""
if (Test-Path $skillDocPath) {
    try {
        $skillDocContent = [System.IO.File]::ReadAllText($skillDocPath, [System.Text.Encoding]::UTF8)
        if ($skillDocContent -match '(?m)^[-*]\s*(?:集成分支|Integration branch)[：:]\s*`?([a-zA-Z0-9_\-\.\/]+)`?') {
            $declaredIntegrationBranch = $Matches[1].Trim()
        }
    } catch {}
}

$hasRemote = ((Invoke-Git @("remote", "get-url", "origin")).ExitCode -eq 0)
$IntegrationBranch = Resolve-IntegrationBranch $IntegrationBranch $declaredIntegrationBranch $hasRemote

$integCheck = Invoke-Git @("rev-parse", "--verify", $IntegrationBranch)
if ($integCheck.ExitCode -ne 0) {
    throw "集成分支 '$IntegrationBranch' 不存在。"
}
$integShaShort = (Invoke-Git @("rev-parse", "--short", $IntegrationBranch)).Output

if ($hasRemote -and (-not $NoFetch)) {
    Write-Host "正在拉取远端最新引用 (git fetch --prune)..." -ForegroundColor DarkGray
    Invoke-Git @("fetch", "--prune", "origin") | Out-Null
}

$wtInfo = Get-WorktreeMap
$currentDir = (Get-Location).Path
try { $currentDir = (Resolve-Path $currentDir).Path } catch {}

# 获取所有本地分支
$localBranchRaw = Invoke-Git @("for-each-ref", "--format=%(refname:short)|%(objectname:short)|%(upstream:short)|%(upstream:track)", "refs/heads/")
$localBranches = @{}
if ($localBranchRaw.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($localBranchRaw.Output))) {
    foreach ($line in ($localBranchRaw.Output -split "`r?`n")) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $parts = $line.Split('|')
        $bName = $parts[0].Trim()
        $sha = if ($parts.Length -gt 1) { $parts[1].Trim() } else { "" }
        $up = if ($parts.Length -gt 2) { $parts[2].Trim() } else { "" }
        $track = if ($parts.Length -gt 3) { $parts[3].Trim() } else { "" }
        $localBranches[$bName] = [PSCustomObject]@{
            Name     = $bName
            Sha      = $sha
            Upstream = $up
            Track    = $track
        }
    }
}

# 获取所有远端分支
$remoteBranchRaw = Invoke-Git @("for-each-ref", "--format=%(refname:short)|%(objectname:short)", "refs/remotes/origin/")
$remoteBranches = @{}
if ($remoteBranchRaw.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($remoteBranchRaw.Output))) {
    foreach ($line in ($remoteBranchRaw.Output -split "`r?`n")) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $parts = $line.Split('|')
        $rName = $parts[0].Trim()
        if ($rName -eq "origin/HEAD" -or $rName.StartsWith("origin/HEAD")) { continue }
        $sha = if ($parts.Length -gt 1) { $parts[1].Trim() } else { "" }
        $shortName = if ($rName.StartsWith("origin/")) { $rName.Substring(7) } else { $rName }
        $remoteBranches[$shortName] = [PSCustomObject]@{
            FullName = $rName
            Sha      = $sha
        }
    }
}

# 聚合所有分支名称
$allBranchNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($k in $localBranches.Keys) { $allBranchNames.Add($k) | Out-Null }
foreach ($k in $remoteBranches.Keys) { $allBranchNames.Add($k) | Out-Null }

Write-Host ""
Write-Host "========================== 分支拓扑与净贡献巡检 ==========================" -ForegroundColor Cyan
Write-Host "集成分支: $IntegrationBranch ($integShaShort)" -ForegroundColor Green
Write-Host "当前目录: $currentDir" -ForegroundColor DarkGray
Write-Host "--------------------------------------------------------------------------"

$branchReportList = [System.Collections.Generic.List[PSCustomObject]]::new()
$totalPendingCommits = 0
$branchesWithNetCommits = 0

foreach ($bName in ($allBranchNames | Sort-Object)) {
    $hasLocal = $localBranches.ContainsKey($bName)
    $hasRemoteB = $remoteBranches.ContainsKey($bName)
    $lSha = if ($hasLocal) { $localBranches[$bName].Sha } else { "-" }
    $rSha = if ($hasRemoteB) { $remoteBranches[$bName].Sha } else { "-" }

    # 确定追踪状态
    $remoteStatus = "LocalOnly"
    if ($hasLocal -and $hasRemoteB) {
        if ($lSha -eq $rSha) {
            $remoteStatus = "Synced"
        } else {
            $isAnc = (Invoke-Git @("merge-base", "--is-ancestor", $lSha, $rSha)).ExitCode -eq 0
            $isRAnc = (Invoke-Git @("merge-base", "--is-ancestor", $rSha, $lSha)).ExitCode -eq 0
            if ($isAnc) {
                $aheadCount = (Invoke-Git @("rev-list", "--count", "$lSha..$rSha")).Output
                $remoteStatus = "Behind ($aheadCount)"
            } elseif ($isRAnc) {
                $behindCount = (Invoke-Git @("rev-list", "--count", "$rSha..$lSha")).Output
                $remoteStatus = "Ahead ($behindCount)"
            } else {
                $remoteStatus = "Diverged"
            }
        }
    } elseif ($hasRemoteB -and (-not $hasLocal)) {
        $remoteStatus = "RemoteOnly"
    }

    # 确定 Worktree 占用
    $wtStatus = "-"
    if ($wtInfo.BranchToPath.ContainsKey($bName)) {
        $p = $wtInfo.BranchToPath[$bName]
        if ($p -eq $currentDir) {
            $wtStatus = "Current"
        } else {
            $wtStatus = "Worktree"
        }
    }

    # 确定净贡献 (git cherry -v against IntegrationBranch)
    $netCount = 0
    $netList = [System.Collections.Generic.List[string]]::new()
    $evalRef = if ($hasLocal) { $bName } else { "origin/$bName" }
    if ($bName -ne $IntegrationBranch) {
        $cherryRes = Invoke-Git @("cherry", "-v", $IntegrationBranch, $evalRef)
        if ($cherryRes.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($cherryRes.Output))) {
            foreach ($cline in ($cherryRes.Output -split "`r?`n")) {
                if ($cline -match '^\+\s+([0-9a-fA-F]+)\s+(.*)$') {
                    $netCount++
                    $netList.Add("$($Matches[1].Substring(0, [Math]::Min(7, $Matches[1].Length))) $($Matches[2])")
                }
            }
        }
    }

    if ($netCount -gt 0) {
        $branchesWithNetCommits++
        $totalPendingCommits += $netCount
    }

    $branchReportList.Add([PSCustomObject]@{
        Branch       = $bName
        IsInteg      = ($bName -eq $IntegrationBranch)
        LocalSha     = $lSha
        RemoteStatus = $remoteStatus
        NetCommits   = $netCount
        NetList      = $netList
        Worktree     = $wtStatus
    })
}

# 格式化表格打印
$fmt = "{0,-24} {1,-10} {2,-16} {3,-12} {4,-10}"
Write-Host ([string]::Format($fmt, "分支名称", "本地 SHA", "远端对齐状态", "净贡献提交", "Worktree")) -ForegroundColor DarkGray
Write-Host ("-" * 74) -ForegroundColor DarkGray

foreach ($row in $branchReportList) {
    $netStr = if ($row.IsInteg) { "(集成分支)" } elseif ($row.NetCommits -eq 0) { "0 (已对齐)" } else { "+$($row.NetCommits) (待同步)" }
    $lineStr = [string]::Format($fmt, $row.Branch, $row.LocalSha, $row.RemoteStatus, $netStr, $row.Worktree)
    
    if ($row.IsInteg) {
        Write-Host $lineStr -ForegroundColor Cyan
    } elseif ($row.NetCommits -gt 0) {
        Write-Host $lineStr -ForegroundColor Yellow
    } elseif ($row.RemoteStatus -eq "Diverged") {
        Write-Host $lineStr -ForegroundColor Red
    } else {
        Write-Host $lineStr -ForegroundColor White
    }

    if ($Detailed -and $row.NetList.Count -gt 0) {
        foreach ($nl in $row.NetList) {
            Write-Host "   + $nl" -ForegroundColor DarkYellow
        }
    }
}

Write-Host "--------------------------------------------------------------------------"
Write-Host "统计: 共扫描 $($branchReportList.Count) 个分支 | $branchesWithNetCommits 个分支有净提交 | 累计待合入净提交 $totalPendingCommits 个" -ForegroundColor Cyan
if ($totalPendingCommits -gt 0) {
    Write-Host "⚡ 推荐一键 1-Shot 闭环同步所有分支:" -ForegroundColor Yellow
    Write-Host "   pwsh .agents/skills/git-workflow/scripts/sync-branch.ps1 -Apply" -ForegroundColor White
} else {
    Write-Host "✅ 所有分支均已包含在集成分支中，无需同步。" -ForegroundColor Green
}
Write-Host "==========================================================================" -ForegroundColor Cyan
Write-Host ""