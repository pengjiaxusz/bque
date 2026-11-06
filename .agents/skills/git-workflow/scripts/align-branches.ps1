<#
.SYNOPSIS
    批量分支与 Worktree 对齐及安全推送工具 (Align Branches)

.DESCRIPTION
    将指定或全部本地分支及关联 Worktree 批量重置对齐至目标提交点（默认当前集成分支 Tip），
    并使用 --force-with-lease 安全推送到远端。
    Worktree 占用的分支自动在对应工作树目录下执行 git reset --hard，非占用分支执行 git branch -f。

.PARAMETER TargetCommit
    对齐目标提交或分支名。默认自动推导为当前目录激活的集成分支。

.PARAMETER Branches
    需要对齐的分支名称列表。若未指定，对齐除目标分支外的所有本地分支。

.PARAMETER Apply
    是否执行实际重置与推送。未指定时仅预检打印。

.PARAMETER NoPush
    跳过推送。
#>

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$TargetCommit = "",

    [Parameter(Position = 1)]
    [string[]]$Branches = @(),

    [switch]$Apply,
    [switch]$NoPush,
    [switch]$Force
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

function Get-RemoteNames {
    $res = Invoke-Git @("remote")
    $names = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    if ($res.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($res.Output))) {
        foreach ($line in ($res.Output -split "`r?`n")) {
            $t = $line.Trim()
            if (-not [string]::IsNullOrWhiteSpace($t)) { $names.Add($t) | Out-Null }
        }
    }
    if ($names.Count -eq 0) { $names.Add("origin") | Out-Null }
    return $names
}

function Test-ValidBranchName {
    param(
        [string]$BranchName,
        [System.Collections.Generic.HashSet[string]]$RemoteNames = $null
    )
    if ([string]::IsNullOrWhiteSpace($BranchName)) { return $false }
    $b = $BranchName.Trim()

    # 1. 绝不能等于任何远程仓库名（如 origin, upstream）
    if ($RemoteNames -and $RemoteNames.Contains($b)) { return $false }

    # 2. 绝不能包含底层 ref 命名空间或递归前缀（heads/..., refs/..., remotes/...）
    if ($b.StartsWith("heads/", [System.StringComparison]::OrdinalIgnoreCase) -or
        $b.StartsWith("refs/", [System.StringComparison]::OrdinalIgnoreCase) -or
        $b.StartsWith("remotes/", [System.StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }

    # 3. 绝不能包含内部备份命名空间
    if ($b.StartsWith("sync-backup/", [System.StringComparison]::OrdinalIgnoreCase) -or
        $b.StartsWith("cleanup-backup/", [System.StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }

    # 4. 绝不能包含套娃或递归路径片段（如包含 /heads/、/refs/、/remotes/）
    if ($b -match "(?i)(?:^|/)(?:refs|heads|remotes)(?:/|$)") {
        return $false
    }

    # 5. 绝不能以 remote/ 开头（如 origin/main，属于远程跟踪引用而非本地特性分支名）
    if ($RemoteNames) {
        foreach ($r in $RemoteNames) {
            if ($b.StartsWith("$r/", [System.StringComparison]::OrdinalIgnoreCase)) {
                return $false
            }
        }
    }

    # 6. 特殊保留名
    if ($b -eq "HEAD" -or $b.StartsWith("HEAD/", [System.StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }

    # 7. Git check-ref-format 校验
    $checkRes = Invoke-Git @("check-ref-format", "--branch", $b)
    if ($checkRes.ExitCode -ne 0) {
        return $false
    }

    return $true
}

function Get-CleanBranchName {
    param(
        [string]$RefOrBranch,
        [System.Collections.Generic.HashSet[string]]$RemoteNames = $null
    )
    if ([string]::IsNullOrWhiteSpace($RefOrBranch)) { return "" }
    $clean = $RefOrBranch.Trim()

    # 逐层剥离前缀（防止意外传入 refs/heads/ 或 refs/remotes/）
    while ($clean.StartsWith("refs/heads/", [System.StringComparison]::OrdinalIgnoreCase) -or
           $clean.StartsWith("refs/remotes/", [System.StringComparison]::OrdinalIgnoreCase)) {
        if ($clean.StartsWith("refs/heads/", [System.StringComparison]::OrdinalIgnoreCase)) {
            $clean = $clean.Substring(11).Trim()
        } elseif ($clean.StartsWith("refs/remotes/", [System.StringComparison]::OrdinalIgnoreCase)) {
            $clean = $clean.Substring(13).Trim()
        }
    }

    if ($RemoteNames) {
        foreach ($r in $RemoteNames) {
            if ($clean.StartsWith("$r/", [System.StringComparison]::OrdinalIgnoreCase)) {
                $clean = $clean.Substring($r.Length + 1).Trim()
            }
        }
    }

    if (Test-ValidBranchName $clean $RemoteNames) {
        return $clean
    }
    return ""
}

function Resolve-IntegrationBranch {
    param(
        [string]$ExplicitBranch,
        [string]$DeclaredBranch,
        [bool]$HasRemote,
        [System.Collections.Generic.HashSet[string]]$RemoteNames = $null
    )

    if (-not [string]::IsNullOrWhiteSpace($ExplicitBranch)) {
        $cExplicit = Get-CleanBranchName $ExplicitBranch $RemoteNames
        if (-not [string]::IsNullOrWhiteSpace($cExplicit)) {
            return $cExplicit
        }
        throw "显式指定的集成分支名称非法或无效: '$ExplicitBranch'"
    }

    $symRef = Invoke-Git @("symbolic-ref", "-q", "HEAD")
    if ($symRef.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($symRef.Output))) {
        $trimmed = $symRef.Output.Trim()
        if ($trimmed.StartsWith("refs/heads/")) {
            $cand = Get-CleanBranchName ($trimmed.Substring(11)) $RemoteNames
            if (-not [string]::IsNullOrWhiteSpace($cand)) {
                Write-Host "💡 自动选定当前目录激活的分支作为集成分支: '$cand'" -ForegroundColor Cyan
                return $cand
            }
        }
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
                $cand = Get-CleanBranchName ($line.Substring(18)) $RemoteNames
                if ($wPath -eq $currentPath -and (-not [string]::IsNullOrWhiteSpace($cand))) {
                    Write-Host "💡 检测到当前 Worktree 对应的登记分支作为集成分支: '$cand'" -ForegroundColor Cyan
                    return $cand
                }
            }
        }
    }

    $pointsAt = Invoke-Git @("branch", "--points-at", "HEAD", "--format=%(refname)")
    if ($pointsAt.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($pointsAt.Output))) {
        $candidates = [System.Collections.Generic.List[string]]::new()
        foreach ($pLine in ($pointsAt.Output -split "`r?`n")) {
            $pClean = $pLine.Trim()
            if ($pClean.StartsWith("refs/heads/")) {
                $c = Get-CleanBranchName ($pClean.Substring(11)) $RemoteNames
                if (-not [string]::IsNullOrWhiteSpace($c) -and (-not $candidates.Contains($c))) {
                    $candidates.Add($c)
                }
            }
        }
        if ($candidates.Count -eq 1) {
            Write-Host "💡 检测到指向当前 HEAD 的唯一本地分支作为集成分支: '$($candidates[0])'" -ForegroundColor Cyan
            return $candidates[0]
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($DeclaredBranch)) {
        $cDeclared = Get-CleanBranchName $DeclaredBranch $RemoteNames
        if (-not [string]::IsNullOrWhiteSpace($cDeclared)) {
            Write-Host "💡 根据 SKILL.md 登记选定集成分支: '$cDeclared'" -ForegroundColor Cyan
            return $cDeclared
        }
    }

    if ($HasRemote) {
        $rHead = Invoke-Git @("symbolic-ref", "-q", "refs/remotes/origin/HEAD")
        if ($rHead.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($rHead.Output))) {
            $trimmed = $rHead.Output.Trim()
            if ($trimmed.StartsWith("refs/remotes/origin/")) {
                $rh = Get-CleanBranchName ($trimmed.Substring("refs/remotes/origin/".Length)) $RemoteNames
                if (-not [string]::IsNullOrWhiteSpace($rh)) {
                    Write-Host "💡 根据 origin/HEAD 选定集成分支: '$rh'" -ForegroundColor Cyan
                    return $rh
                }
            }
        }
        foreach ($b in @("main", "dev", "develop", "master", "trunk")) {
            if ((Invoke-Git @("rev-parse", "--verify", "origin/$b")).ExitCode -eq 0) {
                Write-Host "💡 探测到远端常用集成分支: '$b'" -ForegroundColor Cyan
                return $b
            }
        }
    }

    foreach ($b in @("main", "dev", "develop", "master", "trunk")) {
        if ((Invoke-Git @("rev-parse", "--verify", $b)).ExitCode -eq 0) {
            Write-Host "💡 探测到本地常用集成分支: '$b'" -ForegroundColor Cyan
            return $b
        }
    }

    $dConf = Invoke-Git @("config", "init.defaultBranch")
    if ($dConf.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($dConf.Output))) {
        $cDef = Get-CleanBranchName ($dConf.Output.Trim()) $RemoteNames
        if (-not [string]::IsNullOrWhiteSpace($cDef)) {
            return $cDef
        }
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

function Get-DirtyItems {
    param(
        [System.Collections.Generic.List[string]]$AllWorktreePaths,
        [System.Collections.Generic.List[string]]$DeclaredPatterns,
        [string]$WorkingDir = ""
    )
    $cmd = if ($WorkingDir) { @("-C", $WorkingDir, "status", "--porcelain") } else { @("status", "--porcelain") }
    $res = Invoke-Git $cmd
    if ($res.ExitCode -ne 0) {
        throw "无法获取 Git 仓库状态: $($res.Error)"
    }
    $dirty = [System.Collections.Generic.List[string]]::new()
    if (-not [string]::IsNullOrWhiteSpace($res.Output)) {
        $currentResolved = if ($WorkingDir) { (Resolve-Path $WorkingDir).Path } else { (Resolve-Path (Get-Location).Path).Path }
        foreach ($sl in ($res.Output -split "`r?`n")) {
            if ([string]::IsNullOrWhiteSpace($sl)) { continue }
            $relPath = $sl.Substring(3).Trim().Trim('"').TrimEnd('/')
            $absCandidate = Join-Path $currentResolved $relPath
            try { if (Test-Path $absCandidate) { $absCandidate = (Resolve-Path $absCandidate).Path } } catch {}

            $isRegisteredWt = $false
            foreach ($wt in $AllWorktreePaths) {
                if ($wt -eq $absCandidate) {
                    $isRegisteredWt = $true
                    break
                }
            }
            if ($isRegisteredWt) { continue }

            $isIgnored = $false
            foreach ($pat in $DeclaredPatterns) {
                if ($relPath -match $pat) {
                    $isIgnored = $true
                    break
                }
            }
            if ($isIgnored) { continue }

            $dirty.Add($sl)
        }
    }
    return $dirty
}

$skillDocPath = Join-Path $PSScriptRoot "..\SKILL.md"
if (-not (Test-Path $skillDocPath)) {
    $candidate = Join-Path (Get-Location).Path ".agents\skills\git-workflow\SKILL.md"
    if (Test-Path $candidate) {
        $skillDocPath = $candidate
    }
}
$declaredDirtyPatterns = [System.Collections.Generic.List[string]]::new()
if (Test-Path $skillDocPath) {
    try {
        $skillDocContent = [System.IO.File]::ReadAllText($skillDocPath, [System.Text.Encoding]::UTF8)
        $patRegex = '(?m)^[-*]\s*(?:忽略未追踪路径正则|忽略路径正则|忽略正则|Dirty ignore regex|Dirty ignore pattern)[：:]\s*`?([^\r\n`]+)`?'
        $patMatches = [regex]::Matches($skillDocContent, $patRegex)
        foreach ($m in $patMatches) {
            $patVal = $m.Groups[1].Value.Trim()
            if (-not [string]::IsNullOrWhiteSpace($patVal)) {
                $declaredDirtyPatterns.Add($patVal)
            }
        }
    } catch {}
}

$remoteNames = Get-RemoteNames
$hasRemote = ((Invoke-Git @("remote", "get-url", "origin")).ExitCode -eq 0)
if ([string]::IsNullOrWhiteSpace($TargetCommit)) {
    $TargetCommit = Resolve-IntegrationBranch "" "" $hasRemote $remoteNames
}

$targetShaRes = Invoke-Git @("rev-parse", $TargetCommit)
if ($targetShaRes.ExitCode -ne 0) {
    throw "无法解析对齐目标提交 '$TargetCommit': $($targetShaRes.Error)"
}
$targetSha = $targetShaRes.Output
$targetShaShort = $targetSha.Substring(0, [Math]::Min(7, $targetSha.Length))

$wtInfo = Get-WorktreeMap
$currentDir = (Get-Location).Path
try { $currentDir = (Resolve-Path $currentDir).Path } catch {}

# 解析要对齐的分支列表
$branchesToAlign = [System.Collections.Generic.List[string]]::new()
if ($Branches.Count -gt 0) {
    foreach ($b in $Branches) {
        $clean = Get-CleanBranchName $b $remoteNames
        if (-not [string]::IsNullOrWhiteSpace($clean)) {
            $branchesToAlign.Add($clean)
        }
    }
} else {
    $allLocalRefs = Invoke-Git @("for-each-ref", "--format=%(refname)", "refs/heads/")
    if ($allLocalRefs.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($allLocalRefs.Output))) {
        foreach ($line in ($allLocalRefs.Output -split "`r?`n")) {
            $lineClean = $line.Trim()
            if ($lineClean.StartsWith("refs/heads/")) {
                $rawB = $lineClean.Substring(11).Trim()
                $cleanB = Get-CleanBranchName $rawB $remoteNames
                if (-not [string]::IsNullOrWhiteSpace($cleanB) -and $cleanB -ne $TargetCommit) {
                    $branchesToAlign.Add($cleanB)
                }
            }
        }
    }
}

if ($branchesToAlign.Count -eq 0) {
    Write-Host "ℹ️ 没有需要对齐的分支。" -ForegroundColor Yellow
    return
}

Write-Host ""
Write-Host "========================== 批量分支对齐 ==========================" -ForegroundColor Cyan
Write-Host "目标基线: $TargetCommit ($targetShaShort)" -ForegroundColor Green
Write-Host "目标分支数: $($branchesToAlign.Count)" -ForegroundColor White
Write-Host "------------------------------------------------------------------"

$previewList = [System.Collections.Generic.List[PSCustomObject]]::new()
foreach ($b in $branchesToAlign) {
    $currSha = (Invoke-Git @("rev-parse", "--short", $b)).Output
    $isWt = $wtInfo.BranchToPath.ContainsKey($b)
    $wtPath = if ($isWt) { $wtInfo.BranchToPath[$b] } else { "-" }
    $action = if ($isWt) { "Worktree Reset" } else { "Branch Ref Update" }

    $previewList.Add([PSCustomObject]@{
        Branch   = $b
        Current  = $currSha
        Action   = $action
        Worktree = $wtPath
    })
}

$fmt = "{0,-22} {1,-10} {2,-20} {3}"
Write-Host ([string]::Format($fmt, "分支名称", "当前 SHA", "重置途径", "Worktree 路径")) -ForegroundColor DarkGray
Write-Host ("-" * 68) -ForegroundColor DarkGray
foreach ($item in $previewList) {
    Write-Host ([string]::Format($fmt, $item.Branch, $item.Current, $item.Action, $item.Worktree)) -ForegroundColor White
}
Write-Host "------------------------------------------------------------------"

if (-not $Apply) {
    Write-Host "⚠️ 当前为预检模式 (Dry Run)，未实际执行任何重置与推送。" -ForegroundColor Yellow
    Write-Host "执行对齐与推送请加 -Apply:" -ForegroundColor Cyan
    Write-Host "  pwsh .agents/skills/git-workflow/scripts/align-branches.ps1 -TargetCommit '$TargetCommit' -Apply" -ForegroundColor White
    Write-Host "==================================================================" -ForegroundColor Cyan
    Write-Host ""
    return
}

Write-Host ">> 开始批量执行对齐..." -ForegroundColor Cyan
$dirtyCurr = Get-DirtyItems $wtInfo.AllPaths $declaredDirtyPatterns
if ($dirtyCurr.Count -gt 0) {
    throw "当前工作区存在未提交改动，严禁执行对齐：`n$($dirtyCurr -join "`n")"
}
foreach ($item in $previewList) {
    if ($item.Worktree -ne "-" -and $item.Worktree -ne $currentDir) {
        $wtDirty = Get-DirtyItems $wtInfo.AllPaths $declaredDirtyPatterns $item.Worktree
        if ($wtDirty.Count -gt 0) {
            throw "占用分支 '$($item.Branch)' 的 Worktree ($($item.Worktree)) 存在未提交改动，严禁强制重置：`n$($wtDirty -join "`n")"
        }
    }
}
foreach ($item in $previewList) {
    $b = $item.Branch
    if ($wtInfo.BranchToPath.ContainsKey($b)) {
        $wPath = $wtInfo.BranchToPath[$b]
        if ($wPath -eq $currentDir) {
            $r = Invoke-Git @("reset", "--hard", $targetSha)
            if ($r.ExitCode -ne 0) { throw "重置当前工作区失败: $($r.Error)" }
        } else {
            $r = Invoke-Git @("-C", $wPath, "reset", "--hard", $targetSha)
            if ($r.ExitCode -ne 0) { throw "重置 Worktree ($wPath) 失败: $($r.Error)" }
        }
    } else {
        $r = Invoke-Git @("branch", "-f", $b, $targetSha)
        if ($r.ExitCode -ne 0) { throw "更新分支指针 $b 失败: $($r.Error)" }
    }
    Write-Host "  ✅ $b 已对齐至 $targetShaShort" -ForegroundColor Green
}

if ($hasRemote -and (-not $NoPush)) {
    Write-Host ">> 正在安全推送已对齐分支至远端 (--force-with-lease)..." -ForegroundColor Cyan
    $pushBranches = @($branchesToAlign | Where-Object { Test-ValidBranchName $_ $remoteNames })
    $pushArgs = @("push", "--force-with-lease", "origin") + $pushBranches
    $pRes = Invoke-Git $pushArgs
    if ($pRes.ExitCode -ne 0) {
        Write-Warning "批量推送告警: $($pRes.Error)，正在逐个分支安全补推..."
        foreach ($b in $branchesToAlign) {
            $subPush = Invoke-Git @("push", "--force-with-lease", "origin", $b)
            if ($subPush.ExitCode -eq 0) {
                Write-Host "  ✅ $b 已推送到 origin" -ForegroundColor Green
            } else {
                Write-Warning "  ⚠️ $b 推送失败: $($subPush.Error)"
            }
        }
    } else {
        Write-Host "  ✅ 全部 $($branchesToAlign.Count) 个分支已安全推送到 origin" -ForegroundColor Green
    }
}

Write-Host "================== 批量分支对齐完成 ==================" -ForegroundColor Green
Write-Host ""
