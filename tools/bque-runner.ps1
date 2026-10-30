# tools/bque-runner.ps1 - bque CLI 极简转发垫层（向后兼容）
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$Status,
    [switch]$Clean,
    [switch]$Pause,
    [switch]$Resume,
    [switch]$Bench,
    [string]$Desc = "",
    [string]$Priority = "",
    [int]$Timeout = 0,
    [string]$Cmd = "",
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CommandArgs = @()
)

if ($Status) { & bque status; exit $LASTEXITCODE }
if ($Clean)  { & bque clean; exit $LASTEXITCODE }
if ($Pause)  { & bque pause; exit $LASTEXITCODE }
if ($Resume) { & bque resume; exit $LASTEXITCODE }

$argsList = if ($Bench) { @("bench") } else { @("run") }
if ($Desc) { $argsList += @("--desc", $Desc) }
if ($Priority) { $argsList += @("--priority", $Priority) }
if ($Timeout -gt 0) { $argsList += @("--timeout", $Timeout.ToString()) }

$target = if ($Cmd) { @("pwsh", "-NoProfile", "-Command", $Cmd) } else { $CommandArgs }
if ($target.Count -gt 0 -and $target[0] -eq "--") {
    $target = if ($target.Count -gt 1) { $target[1..($target.Count - 1)] } else { @() }
}

& bque @argsList -- @target
exit $LASTEXITCODE
