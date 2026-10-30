# tools/bque-runner.ps1
#
# bque 全局编译调度与跑分保护代理
#
# 核心功能：
#   1. 多工作树全局排队：接入 bque 全局互斥锁，避免多项目并发编译撑爆 CPU 与磁盘 IO；
#   2. 进程优先级自动降级：默认以 Idle 优先级启动任务，保证前台交互永远丝滑流畅；
#   3. 纯净跑分模式：通过 bque bench 独占执行，秒级挂起后台构建，提供干净的 CPU 运行环境；
#   4. 防递归嵌套死锁：检测 BQUE_ACTIVE_PID 环境变量，若已处于 bque 监督下则直通执行。
#
# 用法：
#   pwsh tools/bque-runner.ps1 -Desc "Cargo Build" -Cmd "cargo build"
#   pwsh tools/bque-runner.ps1 -Desc "Cargo Clippy" -Priority below-normal -Cmd "cargo clippy -- -D warnings"
#   pwsh tools/bque-runner.ps1 -Bench -Desc "Benchmark" -Cmd "cargo bench"
#   pwsh tools/bque-runner.ps1 -Status
#   pwsh tools/bque-runner.ps1 -Clean
#   pwsh tools/bque-runner.ps1 -Pause
#   pwsh tools/bque-runner.ps1 -Resume

[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $false)]
    [switch]$Status,

    [Parameter(Mandatory = $false)]
    [switch]$Clean,

    [Parameter(Mandatory = $false)]
    [switch]$Pause,

    [Parameter(Mandatory = $false)]
    [switch]$Resume,

    [Parameter(Mandatory = $false)]
    [switch]$Bench,

    [Parameter(Mandatory = $false)]
    [string]$Desc = "",

    [Parameter(Mandatory = $false)]
    [ValidateSet("", "idle", "below-normal", "normal", "high")]
    [string]$Priority = "",

    [Parameter(Mandatory = $false)]
    [int]$Timeout = 0,

    [Parameter(Mandatory = $false)]
    [string]$Cmd = "",

    [Parameter(Mandatory = $false, ValueFromRemainingArguments = $true)]
    [string[]]$CommandArgs = @()
)

$ErrorActionPreference = "Stop"

# 1. 检查 bque 是否存在于系统 PATH
$bqueCmd = Get-Command "bque" -ErrorAction SilentlyContinue
if (-not $bqueCmd) {
    Write-Host ""
    Write-Host "================================================================" -ForegroundColor Red
    Write-Host "   ❌ [致命缺失] 未在系统 PATH 中找到 bque 全局编译调度器！      " -ForegroundColor Red
    Write-Host "================================================================" -ForegroundColor Red
    Write-Host ""
    Write-Host "本项目已接入 bque 调度引擎进行全局多工作树编译排队、CPU 动态限流与跑分保护。" -ForegroundColor Yellow
    Write-Host "请前往最新 Release 下载 bque.exe 并将其所在目录添加到系统环境变量 PATH：" -ForegroundColor Cyan
    Write-Host "👉 https://github.com/pengjiaxusz/bque/releases/latest" -ForegroundColor Green
    Write-Host ""
    exit 1
}

# 2. 状态查询与运维控制
if ($Status) {
    & bque status
    exit $LASTEXITCODE
}

if ($Clean) {
    & bque clean
    exit $LASTEXITCODE
}

if ($Pause) {
    & bque pause
    exit $LASTEXITCODE
}

if ($Resume) {
    & bque resume
    exit $LASTEXITCODE
}

# 3. 规范化待执行命令：优先使用 -Cmd 字符串，其次回退至 CommandArgs
$targetExecArgs = @()
$isScriptCmd = $false

if (-not [string]::IsNullOrWhiteSpace($Cmd)) {
    $targetExecArgs = @("pwsh", "-NoProfile", "-Command", $Cmd)
    $isScriptCmd = $true
} else {
    if ($CommandArgs.Count -gt 0 -and $CommandArgs[0] -eq "--") {
        $CommandArgs = if ($CommandArgs.Count -gt 1) { $CommandArgs[1..($CommandArgs.Count - 1)] } else { @() }
    }
    if ($CommandArgs.Count -eq 0) {
        Write-Error "未指定需要由 bque 调度的命令。用法: pwsh tools/bque-runner.ps1 [-Bench] [-Desc ...] -Cmd '<命令字符串>'"
        exit 1
    }
    $targetExecArgs = $CommandArgs
}

# 4. 执行分支：已处于 bque 会话下（防止多层递归进队死锁）
if (-not [string]::IsNullOrWhiteSpace($env:BQUE_ACTIVE_PID)) {
    if ($isScriptCmd) {
        & pwsh -NoProfile -Command $Cmd
    } else {
        $bin = $targetExecArgs[0]
        $binArgs = if ($targetExecArgs.Count -gt 1) { $targetExecArgs[1..($targetExecArgs.Count - 1)] } else { @() }
        & $bin @binArgs
    }
    exit $LASTEXITCODE
}

# 5. 执行分支：基准测试跑分模式（独占排他锁，挂起后台编译）
if ($Bench) {
    $benchArgs = @("bench")
    if (-not [string]::IsNullOrWhiteSpace($Desc)) {
        $benchArgs += @("--desc", $Desc)
    }
    if (-not [string]::IsNullOrWhiteSpace($Priority)) {
        $benchArgs += @("--priority", $Priority)
    }
    if ($Timeout -gt 0) {
        $benchArgs += @("--timeout", $Timeout.ToString())
    }
    $benchArgs += @("--") + $targetExecArgs
    & bque @benchArgs
    exit $LASTEXITCODE
}

# 6. 执行分支：常规全局排队构建
$runArgs = @("run")
if (-not [string]::IsNullOrWhiteSpace($Desc)) {
    $runArgs += @("--desc", $Desc)
}
$targetPriority = if (-not [string]::IsNullOrWhiteSpace($Priority)) { $Priority } else { "idle" }
$runArgs += @("--priority", $targetPriority)
if ($Timeout -gt 0) {
    $runArgs += @("--timeout", $Timeout.ToString())
}
$runArgs += @("--") + $targetExecArgs

& bque @runArgs
exit $LASTEXITCODE