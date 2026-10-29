<#
.SYNOPSIS
    分层模板同步、批量更新与精准提交流水线 (Template Sync Fast-Track)

.DESCRIPTION
    自动化执行跨项目或单项目的模板同步、巡检与精准提交流程：
    1. 毫秒级探测 pengj-templates-cli 可执行文件（优先本地 target 预编译二进制，次选 PATH，兜底 cargo run）；
    2. 支持批量项目路径（-Projects @(...)）；
    3. 默认预检模式 (Dry Run)，通过 audit 快速诊断各项目对齐与漂移状态；
    4. 指定 -Apply 时一键执行更新，默认开启纯受管技能资产对齐 (-SyncSkills)；
    5. 精准暂存红线守护：严格仅 git add 模板涉及的文件与 manifest，严禁粗暴使用 git add .；
    6. 可选 -Commit 与 -Push，将多项目同步全流程压缩在单次工具调用内完成。

.PARAMETER Projects
    目标项目路径列表。支持多个路径如 @("D:\pengj\repo1", "D:\pengj\repo2")。默认为当前目录 @(".")。

.PARAMETER Apply
    是否实际执行更新操作。未指定时仅进行只读预检诊断 (Audit / Dry Run)。

.PARAMETER SyncSkills
    是否将纯受管技能资产与脚本直接对齐覆盖（即使无托管块也不判定为冲突）。默认 $true。

.PARAMETER Commit
    在 -Apply 更新成功后，是否自动精准暂存并执行 git commit。

.PARAMETER Push
    在 -Commit 提交成功后，是否自动执行 git push。

.PARAMETER CommitMessage
    自定义提交信息。未指定时自动根据约定式提交与项目 commitlint 配置推导（默认: chore(template): 同步分层模板更新）。

.PARAMETER Diff
    预检模式下是否展示详细漂移差异 (diff)。

.EXAMPLE
    # 1. 快速只读预检多个项目的模板漂移状态（毫秒级）
    pwsh .agents/skills/template-sync/scripts/sync-template.ps1 -Projects @("D:\pengj\proj-a", "D:\pengj\proj-b")

    # 2. 一键批量更新并精准提交、推送到远端（1次工具调用全自动完成）
    pwsh .agents/skills/template-sync/scripts/sync-template.ps1 -Projects @("D:\pengj\proj-a", "D:\pengj\proj-b") -Apply -Commit -Push
#>

[CmdletBinding()]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
    [string[]]$Projects = @("."),

    [switch]$Apply,
    [bool]$SyncSkills = $true,
    [switch]$Commit,
    [switch]$Push,
    [string]$CommitMessage = "",
    [switch]$Diff
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# 1. 查找或推导 pengj-templates-cli 最佳执行路径（毫秒级启动）
function Find-CliCommand {
    if ($env:PENGJ_TEMPLATES_CLI -and (Test-Path $env:PENGJ_TEMPLATES_CLI)) {
        return @{ Type = "bin"; Cmd = $env:PENGJ_TEMPLATES_CLI }
    }
    $scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }
    # 向上寻找仓库根目录（探测 Cargo.toml）
    $curr = $scriptDir
    for ($i = 0; $i -lt 6; $i++) {
        if (Test-Path (Join-Path $curr "Cargo.toml")) {
            $relBin = Join-Path $curr "target\release\pengj-templates-cli.exe"
            if (Test-Path $relBin) {
                return @{ Type = "bin"; Cmd = $relBin }
            }
            $dbgBin = Join-Path $curr "target\debug\pengj-templates-cli.exe"
            if (Test-Path $dbgBin) {
                return @{ Type = "bin"; Cmd = $dbgBin }
            }
            break
        }
        $parent = Split-Path -Parent $curr
        if (-not $parent -or $parent -eq $curr) { break }
        $curr = $parent
    }
    $pathBin = Get-Command "pengj-templates-cli" -ErrorAction SilentlyContinue
    if ($pathBin) {
        return @{ Type = "bin"; Cmd = $pathBin.Source }
    }
    return @{ Type = "cargo"; Cmd = "cargo" }
}

$cliInfo = Find-CliCommand

function Invoke-Cli {
    param([string[]]$CommandArgs)
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    if ($cliInfo.Type -eq "bin") {
        $psi.FileName = $cliInfo.Cmd
        $psi.Arguments = ($CommandArgs -join " ")
    } else {
        $psi.FileName = "cargo"
        $psi.Arguments = (@("run", "-q", "-p", "pengj-templates-cli", "--") + $CommandArgs) -join " "
    }
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8

    $p = [System.Diagnostics.Process]::Start($psi)
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()

    return [PSCustomObject]@{
        ExitCode = $p.ExitCode
        Output   = $stdout.Trim()
        Error    = $stderr.Trim()
    }
}

function Invoke-Git {
    param(
        [string]$WorkingDir,
        [string[]]$CommandArgs
    )
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "git"
    $psi.Arguments = ($CommandArgs -join " ")
    $psi.WorkingDirectory = $WorkingDir
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8

    $p = [System.Diagnostics.Process]::Start($psi)
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()

    return [PSCustomObject]@{
        ExitCode = $p.ExitCode
        Output   = $stdout.Trim()
        Error    = $stderr.Trim()
    }
}

function Get-DefaultCommitMessage {
    param([string]$ProjectDir)
    $scope = "template"
    $commitlintPath = Join-Path $ProjectDir "commitlint.config.js"
    if (Test-Path $commitlintPath) {
        $content = Get-Content $commitlintPath -Raw
        if ($content -match "scope-enum" -or $content -match "rules:\s*\{") {
            if ($content -notmatch "['""]template['""]") {
                if ($content -match "['""]ai-skill['""]") {
                    $scope = "ai-skill"
                } else {
                    $scope = ""
                }
            }
        }
    }
    if ($scope) {
        return "chore($scope): 同步分层模板更新"
    } else {
        return "chore: 同步分层模板更新"
    }
}

$summaryRows = @()
$hasAnyViolations = $false

$targetProjects = @()
foreach ($p in $Projects) {
    if ($p -match ",") {
        $targetProjects += ($p -split ",") | ForEach-Object { $_.Trim() }
    } else {
        $targetProjects += $p.Trim()
    }
}
$targetProjects = $targetProjects | Where-Object { $_ }
if ($targetProjects.Count -eq 0) {
    $targetProjects = @(".")
}

Write-Host ">>> 使用 CLI 调用模式: $($cliInfo.Type) ($($cliInfo.Cmd))" -ForegroundColor Cyan

foreach ($rawProj in $targetProjects) {
    if (-not (Test-Path $rawProj)) {
        Write-Warning "项目路径不存在，跳过: $rawProj"
        continue
    }
    $projDir = (Resolve-Path $rawProj).Path
    $manifestPath = Join-Path $projDir ".pengj-templates.json"

    if (-not (Test-Path $manifestPath)) {
        Write-Warning "项目未纳管（缺失 .pengj-templates.json），跳过: $projDir"
        continue
    }

    $projectName = Split-Path -Leaf $projDir

    # ---------------- 模式 1: 预检诊断 (Audit / Dry Run) ----------------
    if (-not $Apply) {
        Write-Host "`n>>> [预检] 巡检项目: $projectName ($projDir)" -ForegroundColor Cyan
        $auditArgs = @("audit", "--dir", "`"$projDir`"", "--json")
        $auditRes = Invoke-Cli -CommandArgs $auditArgs
        if ($auditRes.ExitCode -ne 0 -and -not $auditRes.Output) {
            Write-Error "[$projectName] Audit 失败: $($auditRes.Error)"
            continue
        }

        try {
            $auditJson = $auditRes.Output | ConvertFrom-Json
        } catch {
            Write-Error "[$projectName] 解析 Audit JSON 输出失败: $($auditRes.Output)"
            continue
        }

        $inSync = ($auditJson.items | Where-Object { $_.status -eq "InSync" }).Count
        $customized = ($auditJson.items | Where-Object { $_.status -eq "ProjectCustomized" }).Count
        $upstreamNewer = ($auditJson.items | Where-Object { $_.status -eq "UpstreamNewer" }).Count
        $violations = ($auditJson.items | Where-Object { $_.status -in @("ViolatedManagedBlock", "LocallyModifiedFile") }).Count

        if ($auditJson.has_violations) {
            $hasAnyViolations = $true
            Write-Host "  [警告] 发现违规漂移项: $violations 个！" -ForegroundColor Red
        }

        if ($Diff) {
            $diffArgs = @("audit", "--dir", "`"$projDir`"", "--diff")
            $diffRes = Invoke-Cli -CommandArgs $diffArgs
            Write-Host $diffRes.Output
        }

        $summaryRows += [PSCustomObject]@{
            Project       = $projectName
            Mode          = "DryRun"
            InSync        = $inSync
            Customized    = $customized
            UpstreamNewer = $upstreamNewer
            Violations    = $violations
            Action        = if ($upstreamNewer -gt 0) { "建议运行 -Apply" } else { "已最新" }
        }
        continue
    }

    # ---------------- 模式 2: 执行更新 (-Apply) ----------------
    Write-Host "`n>>> [应用] 正在同步更新: $projectName ($projDir)" -ForegroundColor Cyan
    $updateArgs = @("update", "--dir", "`"$projDir`"")
    if ($SyncSkills) {
        $updateArgs += "--sync-skills"
    }

    $updateRes = Invoke-Cli -CommandArgs $updateArgs
    if ($updateRes.ExitCode -ne 0) {
        Write-Error "[$projectName] 更新失败: $($updateRes.Error) $($updateRes.Output)"
        continue
    }

    Write-Host $updateRes.Output

    # 解析更新涉及的文件列表
    $updatedFiles = @()
    $createdFiles = @()
    $conflictedFiles = @()
    $needsReviewFiles = @()

    foreach ($line in ($updateRes.Output -split "`r?`n")) {
        if ($line -match '^\s*\[更新\]\s+(.+)$') {
            $updatedFiles += $matches[1].Trim()
        } elseif ($line -match '^\s*\[新增\]\s+(.+)$') {
            $createdFiles += $matches[1].Trim()
        } elseif ($line -match '^\s*\[冲突\]\s+(.+)$') {
            $conflictedFiles += $matches[1].Trim()
        } elseif ($line -match '^\s*\[待复核\]\s+(.+)$') {
            $needsReviewFiles += $matches[1].Trim()
        }
    }

    $stagedFiles = @()
    # 精准暂存红线：仅针对受本次更新影响的文件与 manifest 运行 git add
    $candidateFiles = @($updatedFiles + $createdFiles + @(".pengj-templates.json")) | Select-Object -Unique
    foreach ($rel in $candidateFiles) {
        $fullPath = Join-Path $projDir $rel
        if (Test-Path $fullPath) {
            $addRes = Invoke-Git -WorkingDir $projDir -CommandArgs @("add", "`"$rel`"")
            if ($addRes.ExitCode -eq 0) {
                $stagedFiles += $rel
            } else {
                Write-Warning "[$projectName] 暂存文件失败: $rel - $($addRes.Error)"
            }
        }
    }

    $commitStatus = "跳过提交"
    $pushStatus = "跳过推送"

    if ($Commit) {
        $cachedDiff = Invoke-Git -WorkingDir $projDir -CommandArgs @("diff", "--cached", "--name-only")
        if ($cachedDiff.Output) {
            $msg = if ($CommitMessage) { $CommitMessage } else { Get-DefaultCommitMessage -ProjectDir $projDir }
            Write-Host "  -> 精准提交改动: $msg" -ForegroundColor Yellow
            $cRes = Invoke-Git -WorkingDir $projDir -CommandArgs @("commit", "-m", "`"$msg`"")
            if ($cRes.ExitCode -ne 0) {
                Write-Error "[$projectName] git commit 失败: $($cRes.Error) $($cRes.Output)"
                $commitStatus = "提交失败"
            } else {
                $commitStatus = "已提交"
                if ($Push) {
                    Write-Host "  -> 推送到远端仓库..." -ForegroundColor Yellow
                    $pRes = Invoke-Git -WorkingDir $projDir -CommandArgs @("push")
                    if ($pRes.ExitCode -ne 0) {
                        Write-Error "[$projectName] git push 失败: $($pRes.Error) $($pRes.Output)"
                        $pushStatus = "推送失败"
                    } else {
                        $pushStatus = "已推送"
                    }
                }
            }
        } else {
            $commitStatus = "无暂存变更"
            $pushStatus = "无变更"
        }
    }

    $summaryRows += [PSCustomObject]@{
        Project      = $projectName
        Mode         = "Apply"
        Updated      = $updatedFiles.Count
        Created      = $createdFiles.Count
        Conflicted   = $conflictedFiles.Count
        NeedsReview  = $needsReviewFiles.Count
        CommitStatus = $commitStatus
        PushStatus   = $pushStatus
    }
}

Write-Host "`n========================= 执行结果汇总 =========================" -ForegroundColor Green
$summaryRows | Format-Table -AutoSize

if ($hasAnyViolations) {
    Write-Host "[提示] 存在违规篡改项，请遵循规范修复或上报模板通用化！`n" -ForegroundColor Yellow
}