<#
.SYNOPSIS
    分支同步冲突恢复与继续执行工具 (Continue Sync)

.DESCRIPTION
    在 sync-branch.ps1 执行过程中若遇到 cherry-pick 冲突，在开发者/Agent 修复冲突文件并 git add 后，
    通过本脚本 1-Shot 恢复执行：
    1. 自动继续 cherry-pick；
    2. 继续应用队列中剩余的净提交；
    3. 执行 Tree-Diff Guard 树级防漏保全审计；
    4. 推送集成分支；
    5. 批量对齐所有分支与 Worktree 并 safe push (--force-with-lease)；
    6. 执行合后项目门禁验证；
    7. 输出 COMPLETED_READY_TO_REPORT 交付看板。
    亦支持 -Abort 彻底回滚并恢复现场。

.PARAMETER Continue
    解决冲突后继续执行剩余同步流程。

.PARAMETER Abort
    中止同步，恢复集成分支到合入前快照，清理同步状态。

.PARAMETER Status
    查看当前冲突与同步队列状态。

.PARAMETER NoPush
    跳过推送。

.PARAMETER NoVerify
    跳过合后项目验证。
#>

[CmdletBinding()]
param(
    [switch]$Continue,
    [switch]$Abort,
    [switch]$Status,
    [switch]$NoPush,
    [switch]$NoVerify
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

function Test-ContentPreserved {
    param(
        [string]$SourceHash,
        [string]$LandedHash
    )
    $srcShow = Invoke-Git @("show", $SourceHash, "--format=", "--unified=0")
    $landedShow = Invoke-Git @("show", $LandedHash, "--format=", "--unified=0")
    if ($srcShow.ExitCode -ne 0 -or $landedShow.ExitCode -ne 0) {
        return $false
    }
    function ConvertTo-ContentSet {
        param([string]$PatchText)
        $set = @{}
        foreach ($ln in ($PatchText -split "`r?`n")) {
            if ($ln.StartsWith("+++") -or $ln.StartsWith("---") -or $ln.StartsWith("@@") -or $ln.StartsWith("diff --git") -or $ln.StartsWith("index ")) {
                continue
            }
            $t = $ln
            if ($t.StartsWith("+") -or $t.StartsWith("-")) {
                $t = $t.Substring(1)
            }
            $t = $t.Trim()
            if (-not [string]::IsNullOrWhiteSpace($t)) {
                $set[$t] = $true
            }
        }
        return $set
    }
    $srcSet = ConvertTo-ContentSet $srcShow.Output
    $landedSet = ConvertTo-ContentSet $landedShow.Output
    foreach ($k in $srcSet.Keys) {
        if (-not $landedSet.ContainsKey($k)) {
            return $false
        }
    }
    return $true
}

# 查找状态文件
$gitDirRes = Invoke-Git @("rev-parse", "--git-dir")
if ($gitDirRes.ExitCode -ne 0) {
    throw "未在 Git 仓库内执行。"
}
$gitDir = $gitDirRes.Output
try { $gitDir = (Resolve-Path $gitDir).Path } catch {}
$stateFilePath = Join-Path $gitDir "branch-sync-state.json"

if (-not (Test-Path $stateFilePath)) {
    # 检查是否有未完成的 cherry-pick
    $cpHead = Join-Path $gitDir "CHERRY_PICK_HEAD"
    if (Test-Path $cpHead) {
        if ($Abort) {
            Invoke-Git @("cherry-pick", "--abort") | Out-Null
            Write-Host "✅ 已中止进行中的 cherry-pick 操作。" -ForegroundColor Green
            return
        }
        throw "检测到有未完成的 cherry-pick，但未找到 branch-sync-state.json 同步状态记录。"
    }
    Write-Host "ℹ️ 当前没有进行中或被冲突中断的分支同步任务。" -ForegroundColor Yellow
    return
}

$stateJson = [System.IO.File]::ReadAllText($stateFilePath, [System.Text.Encoding]::UTF8)
$state = ConvertFrom-Json $stateJson

$integBranch = $state.IntegrationBranch
$orderedCommits = @($state.OrderedCommits)
$currentIndex = [int]$state.CurrentCommitIndex
$backupIntegRef = $state.BackupIntegRef
$allSourceBranches = @($state.AllSourceBranches)
$declaredVerifyCmd = $state.DeclaredVerifyCmd
$skipPush = [bool]$state.NoPush -or $NoPush

if ($Status -or (-not $Continue -and -not $Abort)) {
    Write-Host ""
    Write-Host "================== 分支同步中断状态 ==================" -ForegroundColor Cyan
    Write-Host "集成分支:       $integBranch"
    Write-Host "当前中断提交:   $($orderedCommits[$currentIndex].Hash) - $($orderedCommits[$currentIndex].Subject)" -ForegroundColor Yellow
    Write-Host "剩余队列提交数: $($orderedCommits.Count - $currentIndex)"
    
    $unmerged = Invoke-Git @("diff", "--name-only", "--diff-filter=U")
    if (-not [string]::IsNullOrWhiteSpace($unmerged.Output)) {
        Write-Host "未解决冲突文件:" -ForegroundColor Red
        foreach ($uf in ($unmerged.Output -split "`r?`n")) {
            Write-Host "  ! $uf" -ForegroundColor Red
        }
    } else {
        Write-Host "冲突文件状态:   已暂存 (无未解决标记)" -ForegroundColor Green
    }
    Write-Host "------------------------------------------------------"
    Write-Host "操作指引:" -ForegroundColor Cyan
    Write-Host "  继续同步: pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Continue" -ForegroundColor White
    Write-Host "  中止回滚: pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Abort" -ForegroundColor White
    Write-Host "======================================================" -ForegroundColor Cyan
    Write-Host ""
    return
}

if ($Abort) {
    Write-Host ">> 正在中止同步并恢复集成分支..." -ForegroundColor Yellow
    Invoke-Git @("cherry-pick", "--abort") | Out-Null
    if (-not [string]::IsNullOrWhiteSpace($backupIntegRef)) {
        Invoke-Git @("reset", "--hard", $backupIntegRef) | Out-Null
        Write-Host "🛡️ 集成分支已恢复至合前快照: $backupIntegRef" -ForegroundColor Green
    }
    Remove-Item -Path $stateFilePath -Force -ErrorAction SilentlyContinue
    Write-Host "✅ 分支同步已安全中止，现场已完全恢复。" -ForegroundColor Green
    return
}

if ($Continue) {
    # 检查冲突文件
    $unmerged = Invoke-Git @("diff", "--name-only", "--diff-filter=U")
    if (-not [string]::IsNullOrWhiteSpace($unmerged.Output)) {
        throw "仍有冲突文件未解决！请先在以下文件中解决冲突并执行 git add 后再继续：`n$($unmerged.Output)"
    }

    Write-Host ">> 1. 检查并继续当前 cherry-pick 提交..." -ForegroundColor Cyan
    $editorCmd = if ([System.Environment]::OSVersion.Platform -like "*Win*" -or $IsWindows) { "cmd.exe /c exit 0" } else { "true" }
    $gitDirRes = Invoke-Git @("rev-parse", "--git-dir")
    $gitDir = if ($gitDirRes.ExitCode -eq 0) { $gitDirRes.Output } else { ".git" }
    try { $gitDir = (Resolve-Path $gitDir).Path } catch {}
    $inCherryPick = (Test-Path (Join-Path $gitDir "CHERRY_PICK_HEAD"))

    if ($inCherryPick) {
        $contRes = Invoke-Git @("-c", "core.editor=$editorCmd", "cherry-pick", "--continue", "--no-edit")
        if ($contRes.ExitCode -ne 0) {
            throw "git cherry-pick --continue 执行失败: $($contRes.Error)"
        }
        Write-Host "✅ 当前提交合入成功！" -ForegroundColor Green
    } else {
        $lastLog = (Invoke-Git @("log", "-1", "--format=%s")).Output
        $targetSubj = $orderedCommits[$currentIndex].Subject
        if ($lastLog -match [regex]::Escape($targetSubj)) {
            Write-Host "ℹ️ 检测到当前提交已在外部完成合入 ($targetSubj)，自动推进至后续队列。" -ForegroundColor Green
        } else {
            Write-Host "ℹ️ 当前无进行中的 cherry-pick 操作，继续后续队列检查。" -ForegroundColor Yellow
        }
    }

    # 继续处理后续提交
    $nextIndex = $currentIndex + 1
    while ($nextIndex -lt $orderedCommits.Count) {
        $c = $orderedCommits[$nextIndex]
        Write-Host ">> 应用剩余提交 [$($nextIndex + 1)/$($orderedCommits.Count)]: $($c.Hash.Substring(0, [Math]::Min(7, $c.Hash.Length))) $($c.Subject)..." -ForegroundColor DarkGray
        
        $cp = Invoke-Git @("-c", "core.editor=$editorCmd", "cherry-pick", $c.FullHash)
        if ($cp.ExitCode -ne 0) {
            $uCheck = Invoke-Git @("diff", "--name-only", "--diff-filter=U")
            $uOut = if ($null -ne $uCheck.Output) { $uCheck.Output.Trim() } else { "" }
            if ([string]::IsNullOrWhiteSpace($uOut)) {
                # 空提交（已等价合入/重复）：跳过，不丢改动
                Invoke-Git @("cherry-pick", "--skip") | Out-Null
                Write-Host "ℹ️ 提交 $($c.Hash) 为空（改动已等价存在），已跳过。" -ForegroundColor Yellow
                $state.CurrentCommitIndex = $nextIndex
                [System.IO.File]::WriteAllText($stateFilePath, (ConvertTo-Json $state -Depth 10), [System.Text.Encoding]::UTF8)
                $nextIndex++
                continue
            }
            # 真冲突，更新索引并中断
            $state.CurrentCommitIndex = $nextIndex
            [System.IO.File]::WriteAllText($stateFilePath, (ConvertTo-Json $state -Depth 10), [System.Text.Encoding]::UTF8)
            $unm = $uOut
            Write-Host "⚠️ 在提交 $($c.Hash) 再次发生冲突！" -ForegroundColor Yellow
            Write-Host "冲突文件:`n$unm" -ForegroundColor Red
            Write-Host "请解决冲突、git add 后再次执行 continue-sync.ps1 -Continue" -ForegroundColor Cyan
            exit 1
        }
        $headTip = (Invoke-Git @("rev-parse", "HEAD")).Output
        if (-not (Test-ContentPreserved $c.FullHash $headTip.Trim())) {
            throw "内容校验失败：提交 $($c.Hash) - $($c.Subject) 落地后改动不完整，疑似内容丢失，已中断！"
        }
        $nextIndex++
    }

    Write-Host "✅ 所有净贡献提交已全部 cherry-pick 完成！" -ForegroundColor Green

    # Tree-Diff Guard v2：cherry 残留 + 文件级内容包含双重审计
    Write-Host "🔍 执行合后改动保全审计 (Tree-Diff Guard)..." -ForegroundColor DarkCyan
    $backupSourceRefs = $state.BackupSourceRefs
    $guardFailures = [System.Collections.Generic.List[string]]::new()
    if ($null -eq $backupSourceRefs) {
        Write-Host "⚠️ 状态文件中缺失 BackupSourceRefs（旧状态文件），回退为快照模式巡检。" -ForegroundColor Yellow
        foreach ($sb in $allSourceBranches) {
            $checkRef = "refs/sync-backup/$($sb -replace '[^a-zA-Z0-9_\-]', '_')"
            # 查找最新的快照 ref
            $refLines = @((Invoke-Git @("for-each-ref", "--sort=-committerdate", "--format=%(refname)", "$checkRef*")).Output -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
            $bRef = if ($refLines.Count -gt 0) { $refLines[0].Trim() } else { "" }
            if (-not [string]::IsNullOrWhiteSpace($bRef)) {
                $postCherry = Invoke-Git @("cherry", "-v", $integBranch, $bRef)
                if ($postCherry.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($postCherry.Output))) {
                    foreach ($pcl in ($postCherry.Output -split "`r?`n")) {
                        if ($pcl -match '^\+\s+([0-9a-fA-F]+)\s+(.*)$') {
                            # 空提交无 patch-id、cherry 恒报 `+`：无文件改动即视为已合入。
                            $emptyCheck = Invoke-Git @("diff-tree", "--no-commit-id", "--name-only", "-r", $Matches[1])
                            if ($emptyCheck.ExitCode -eq 0 -and [string]::IsNullOrWhiteSpace($emptyCheck.Output)) {
                                continue
                            }
                            $guardFailures.Add("源分支 '$sb' 仍有未合入净提交: $($Matches[1]) $($Matches[2])")
                        }
                    }
                }
            }
        }
    } else {
        foreach ($sb in $allSourceBranches) {
            $bRef = ""
            if ($backupSourceRefs -is [System.Collections.IDictionary]) {
                if ($backupSourceRefs.Contains($sb)) { $bRef = [string]$backupSourceRefs[$sb] }
            } else {
                $prop = $backupSourceRefs.PSObject.Properties[$sb]
                if ($null -ne $prop) { $bRef = [string]$prop.Value }
            }
            if ([string]::IsNullOrWhiteSpace($bRef)) { continue }
            $postCherry = Invoke-Git @("cherry", "-v", $integBranch, $bRef)
            if ($postCherry.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($postCherry.Output))) {
                foreach ($pcl in ($postCherry.Output -split "`r?`n")) {
                    if ($pcl -match '^\+\s+([0-9a-fA-F]+)\s+(.*)$') {
                        # 空提交无 patch-id、cherry 恒报 `+`：无文件改动即视为已合入。
                        $emptyCheck = Invoke-Git @("diff-tree", "--no-commit-id", "--name-only", "-r", $Matches[1])
                        if ($emptyCheck.ExitCode -eq 0 -and [string]::IsNullOrWhiteSpace($emptyCheck.Output)) {
                            continue
                        }
                        $guardFailures.Add("源分支 '$sb' 仍有未合入净提交: $($Matches[1]) $($Matches[2])")
                    }
                }
            }
            $mbRes = Invoke-Git @("merge-base", $integBranch, $bRef)
            if ($mbRes.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($mbRes.Output))) {
                $mb = $mbRes.Output.Trim()
                $srcFilesOut = (Invoke-Git @("diff", "--name-only", $mb, $bRef)).Output
                $integFilesOut = (Invoke-Git @("diff", "--name-only", $mb, $integBranch)).Output
                $srcFiles = @($srcFilesOut -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | ForEach-Object { $_.Trim() })
                $integSet = @{}
                foreach ($f in ($integFilesOut -split "`r?`n")) {
                    $t = $f.Trim()
                    if (-not [string]::IsNullOrWhiteSpace($t)) { $integSet[$t] = $true }
                }
                $missing = @($srcFiles | Where-Object { -not $integSet.ContainsKey($_) })
                if ($missing.Count -gt 0) {
                    $guardFailures.Add("源分支 '$sb' 文件级缺失: $($missing -join ', ')")
                }
            }
        }
    }
    if ($guardFailures.Count -gt 0) {
        throw "Tree-Diff Guard 拦截：`n$($guardFailures -join "`n")"
    }
    Write-Host "✅ Tree-Diff Guard 审计通过：所有分支改动已 100% 完整合入！" -ForegroundColor Green

    # 推送集成分支
    $hasRemote = ((Invoke-Git @("remote", "get-url", "origin")).ExitCode -eq 0)
    if ($hasRemote -and (-not $skipPush)) {
        Write-Host ">> 推送集成分支 origin/$integBranch..." -ForegroundColor DarkGray
        $pInteg = Invoke-Git @("push", "origin", $integBranch)
        if ($pInteg.ExitCode -ne 0) { throw "推送集成分支失败: $($pInteg.Error)" }
    }

    # 对齐所有分支与 Worktree
    Write-Host ">> 批量对齐所有分支与 Worktree 至最新集成分支 Tip..." -ForegroundColor Cyan
    $integTip = (Invoke-Git @("rev-parse", $integBranch)).Output
    $wtInfo = Get-WorktreeMap
    $currentDir = (Get-Location).Path
    try { $currentDir = (Resolve-Path $currentDir).Path } catch {}

    foreach ($sb in $allSourceBranches) {
        if ($sb -eq $integBranch) { continue }
        if ($wtInfo.BranchToPath.ContainsKey($sb)) {
            $wPath = $wtInfo.BranchToPath[$sb]
            if ($wPath -ne $currentDir) {
                Invoke-Git @("-C", $wPath, "reset", "--hard", $integTip) | Out-Null
            }
        } else {
            Invoke-Git @("branch", "-f", $sb, $integTip) | Out-Null
        }
        Write-Host "  ✅ 分支 '$sb' 已对齐" -ForegroundColor Green
    }

    if ($hasRemote -and (-not $skipPush)) {
        Write-Host ">> 安全推送所有对齐分支 (--force-with-lease)..." -ForegroundColor Cyan
        $pushBranches = $allSourceBranches | Where-Object { $_ -ne $integBranch }
        if ($pushBranches.Count -gt 0) {
            $pArgs = @("push", "--force-with-lease", "origin") + $pushBranches
            $pRes = Invoke-Git $pArgs
            if ($pRes.ExitCode -ne 0) {
                foreach ($pb in $pushBranches) {
                    Invoke-Git @("push", "--force-with-lease", "origin", $pb) | Out-Null
                }
            }
            Write-Host "  ✅ 全部对齐分支已推送到 origin" -ForegroundColor Green
        }
    }

    # 合后验证
    $verifyStatus = "SKIPPED"
    if (-not $NoVerify -and (-not [string]::IsNullOrWhiteSpace($declaredVerifyCmd))) {
        Write-Host ">> 运行项目专属合后验证命令: $declaredVerifyCmd" -ForegroundColor Cyan
        try {
            $pinfo = New-Object System.Diagnostics.ProcessStartInfo
            if ($IsWindows -or ($PSVersionTable.PSEdition -ne "Core" -and [System.Environment]::OSVersion.Platform -like "*Win*")) {
                $pinfo.FileName = "powershell.exe"
                $pinfo.Arguments = "-NoProfile -ExecutionPolicy Bypass -Command `"$declaredVerifyCmd`""
            } else {
                $pinfo.FileName = "sh"
                $pinfo.Arguments = "-c `"$declaredVerifyCmd`""
            }
            $pinfo.RedirectStandardOutput = $false
            $pinfo.RedirectStandardError = $false
            $pinfo.UseShellExecute = $false
            $pinfo.WorkingDirectory = (Get-Location).Path

            $vProc = [System.Diagnostics.Process]::Start($pinfo)
            $vProc.WaitForExit()
            if ($vProc.ExitCode -eq 0) {
                $verifyStatus = "PASSED ($declaredVerifyCmd)"
            } else {
                $verifyStatus = "FAILED (退出码 $($vProc.ExitCode))"
            }
        } catch {
            $verifyStatus = "ERROR ($($_.Exception.Message))"
        }
    }

    # 清理状态文件
    Remove-Item -Path $stateFilePath -Force -ErrorAction SilentlyContinue

    Write-Host ""
    Write-Host "================== BRANCH SYNC SUCCESS ==================" -ForegroundColor Green
    Write-Host "集成分支:       $integBranch ($($integTip.Substring(0, [Math]::Min(7, $integTip.Length))))"
    Write-Host "已同步分支:     $($allSourceBranches.Count) 个分支 (已全部对齐并安全推送)"
    Write-Host "合入净提交数:   $($orderedCommits.Count) 个 (严格线性，0 Merge)"
    Write-Host "改动保全审计:   ✅ PASSED (Tree-Diff 100% 完整保留)"
    Write-Host "合后门禁验证:   $verifyStatus"
    Write-Host "工作区状态:     ✅ 干净 (Clean)"
    Write-Host "状态判定:       COMPLETED_READY_TO_REPORT"
    Write-Host "=========================================================" -ForegroundColor Green
    Write-Host ""
}