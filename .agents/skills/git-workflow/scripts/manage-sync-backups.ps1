<#
.SYNOPSIS
    分支同步安全快照管理工具 (Manage Sync Backups)

.DESCRIPTION
    查看、还原或清理 branch-sync 产生的 refs/sync-backup/* 本地持久化快照指针。

.PARAMETER List
    列出所有现存的安全快照。

.PARAMETER RestoreBranch
    要恢复的目标分支名。

.PARAMETER BackupRef
    要恢复到的快照引用 (如 refs/sync-backup/dev/20261007-120000-xxxx)。

.PARAMETER Clean
    清理过期的安全快照。

.PARAMETER OlderThanDays
    清理多少天前的快照（默认 7 天；设为 0 清理全部）。
#>

[CmdletBinding()]
param(
    [switch]$List,
    [string]$RestoreBranch = "",
    [string]$BackupRef = "",
    [switch]$Clean,
    [int]$OlderThanDays = 7,
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

if (-not [string]::IsNullOrWhiteSpace($RestoreBranch) -and -not [string]::IsNullOrWhiteSpace($BackupRef)) {
    Write-Host ">> 正在从安全快照恢复分支 '$RestoreBranch'..." -ForegroundColor Cyan
    $chk = Invoke-Git @("rev-parse", "--verify", $BackupRef)
    if ($chk.ExitCode -ne 0) {
        throw "指定的快照引用不存在: $BackupRef"
    }
    $targetSha = $chk.Output

    # 检查是否被 worktree 占用
    $wtList = Invoke-Git @("worktree", "list", "--porcelain")
    $wtPath = $null
    if ($wtList.ExitCode -eq 0) {
        $lines = $wtList.Output -split "`r?`n"
        $tempP = ""
        foreach ($line in $lines) {
            if ($line.StartsWith("worktree ")) {
                $tempP = $line.Substring(9).Trim()
            } elseif ($line.StartsWith("branch refs/heads/")) {
                if ($line.Substring(18).Trim() -eq $RestoreBranch) {
                    $wtPath = $tempP
                }
            } elseif ([string]::IsNullOrWhiteSpace($line)) {
                $tempP = ""
            }
        }
    }

    if ($wtPath) {
        Write-Host "分支处于 Worktree 中 ($wtPath)，执行 worktree reset..." -ForegroundColor DarkGray
        $r = Invoke-Git @("-C", $wtPath, "reset", "--hard", $targetSha)
        if ($r.ExitCode -ne 0) { throw "重置失败: $($r.Error)" }
    } else {
        $r = Invoke-Git @("branch", "-f", $RestoreBranch, $targetSha)
        if ($r.ExitCode -ne 0) { throw "更新分支指针失败: $($r.Error)" }
    }
    Write-Host "✅ 分支 '$RestoreBranch' 已成功还原至快照 $BackupRef ($targetSha)" -ForegroundColor Green
    return
}

if ($Clean) {
    Write-Host ">> 正在清理过期的安全快照 (早于 $OlderThanDays 天)..." -ForegroundColor Cyan
    $allBackups = Invoke-Git @("for-each-ref", "--format=%(refname)|%(committerdate:unix)", "refs/sync-backup/")
    $nowEpoch = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $cutoff = $nowEpoch - ($OlderThanDays * 86400)
    $deletedCount = 0

    if ($allBackups.ExitCode -eq 0 -and (-not [string]::IsNullOrWhiteSpace($allBackups.Output))) {
        foreach ($line in ($allBackups.Output -split "`r?`n")) {
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            $parts = $line.Split('|')
            $ref = $parts[0].Trim()
            $epoch = [long]0
            if ($parts.Length -gt 1) { [long]::TryParse($parts[1].Trim(), [ref]$epoch) | Out-Null }
            if ($epoch -le $cutoff -or $OlderThanDays -eq 0) {
                Invoke-Git @("update-ref", "-d", $ref) | Out-Null
                $deletedCount++
            }
        }
    }
    Write-Host "✅ 已清理 $deletedCount 个过期快照。" -ForegroundColor Green
    return
}

# 默认模式：列出所有快照
Write-Host ""
Write-Host "===================== refs/sync-backup/ 快照清单 =====================" -ForegroundColor Cyan
$backups = Invoke-Git @("for-each-ref", "--sort=-committerdate", "--format=%(refname)|%(objectname:short)|%(committerdate:iso8601)|%(subject)", "refs/sync-backup/")

if ($backups.ExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($backups.Output)) {
    Write-Host "当前无任何安全快照记录。" -ForegroundColor Yellow
} else {
    $fmt = "{0,-42} {1,-10} {2,-20} {3}"
    Write-Host ([string]::Format($fmt, "快照引用 (Ref)", "SHA", "创建时间", "提交标题")) -ForegroundColor DarkGray
    Write-Host ("-" * 86) -ForegroundColor DarkGray
    foreach ($line in ($backups.Output -split "`r?`n")) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $parts = $line.Split('|')
        $r = $parts[0].Trim()
        $s = if ($parts.Length -gt 1) { $parts[1].Trim() } else { "" }
        $d = if ($parts.Length -gt 2) { $parts[2].Trim() } else { "" }
        $subj = if ($parts.Length -gt 3) { $parts[3].Trim() } else { "" }
        Write-Host ([string]::Format($fmt, $r, $s, $d, $subj)) -ForegroundColor White
    }
}
Write-Host "----------------------------------------------------------------------"
Write-Host "还原命令示例:" -ForegroundColor DarkGray
Write-Host "  pwsh .agents/skills/git-workflow/scripts/manage-sync-backups.ps1 -RestoreBranch 'dev' -BackupRef 'refs/sync-backup/...' " -ForegroundColor White
Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host ""