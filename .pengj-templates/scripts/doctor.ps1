# .pengj-templates/scripts/doctor.ps1 - project environment check.
#
# Checks only the tools the selected template layers actually use, then runs
# every custom check in .pengj-templates/scripts/doctor.d/*.ps1 in sorted order.
#
# Usage:
#   pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1                  report only
#   pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 --fix            also activate the commit hooks
#   pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 --install-cli    download and install pengj-templates-cli
#                                                            (the only step that touches the network;
#                                                            it is never implied by --fix)
#   (the just recipe forwards `just doctor --fix` unchanged on both platforms)
#
# Custom checks: drop *.ps1 files into .pengj-templates/scripts/doctor.d/. Each script runs from
# the project root with DOCTOR_ROOT set to the project root and DOCTOR_FIX=1
# when --fix is given. Exit 0 = pass, non-zero = failure; output is shown as-is.

$Fix = $false
$InstallCli = $false
foreach ($arg in $args) {
    switch ($arg.ToLowerInvariant()) {
        "--fix" { $Fix = $true }
        "-fix" { $Fix = $true }
        "--install-cli" { $InstallCli = $true }
        "-installcli" { $InstallCli = $true }
        "-h" {
            Write-Host "usage: pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 [--fix] [--install-cli]"
            exit 0
        }
        "--help" {
            Write-Host "usage: pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 [--fix] [--install-cli]"
            exit 0
        }
        default {
            Write-Host "[doctor] unknown argument: $arg"
            exit 2
        }
    }
}

$ErrorActionPreference = "Continue"

$root = (& git rev-parse --show-toplevel 2>$null)
if ($LASTEXITCODE -ne 0 -or -not $root) {
    $root = (Get-Location).Path
}
Set-Location $root
$env:DOCTOR_ROOT = $root
if ($Fix) {
    $env:DOCTOR_FIX = "1"
} else {
    Remove-Item Env:DOCTOR_FIX -ErrorAction SilentlyContinue
}

$script:checks = 0
$script:errors = 0
$script:warnings = 0

function Report-Ok($message) {
    $script:checks++
    Write-Host "[doctor] ok    $message" -ForegroundColor Green
}

function Report-Warn($message) {
    $script:checks++
    $script:warnings++
    Write-Host "[doctor] warn  $message" -ForegroundColor Yellow
}

function Report-Fail($message) {
    $script:checks++
    $script:errors++
    Write-Host "[doctor] error $message" -ForegroundColor Red
}

function Test-Tool($name) {
    return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

# --- built-in checks: only what the selected layers use ---------------------

if (Test-Tool git) {
    Report-Ok "git $((& git --version) -replace '^git version ', '')"
} else {
    Report-Fail "git not found"
}


if (Test-Tool just) {
    Report-Ok "just $((& just --version) -replace '^just ', '')"
} else {
    Report-Fail "just not found; install: https://github.com/casey/just#installation"
}



if (Test-Tool cargo) {
    Report-Ok "cargo $((& cargo --version) -replace '^cargo ', '')"
} else {
    Report-Fail "cargo not found; install a Rust toolchain"
}
if (Test-Tool rustc) {
    Report-Ok "rustc $((& rustc --version) -replace '^rustc ', '')"
} else {
    Report-Fail "rustc not found; install a Rust toolchain"
}



if (Test-Tool bque) {
    Report-Ok "bque $((& bque --version 2>$null | Select-Object -First 1))"
} else {
    Report-Warn "bque not found; install: irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex"
}



# --- pengj-templates-cli ---------------------------------------------------
# The commit gate needs it, and the agent-layer workspace links are created by
# its `repo init`, so both layers require it: missing is an error, not a warning.
# Installing is the only network access and needs the explicit --install-cli;
# `--fix` never touches the network.
$CliReleaseBase = "https://github.com/pengjiaxusz/pengj-templates/releases/latest/download"
$CliHint = "irm $CliReleaseBase/install.ps1 | iex"

function Install-CliTool {
# Run the installer in a child process: it exits on its own, and that must
    # not take doctor down with it.
    $installer = Join-Path ([System.IO.Path]::GetTempPath()) "pengj-install-$PID.ps1"
    try {
        Invoke-RestMethod -Uri "$CliReleaseBase/install.ps1" -OutFile $installer -ErrorAction Stop
    } catch {
        Remove-Item $installer -ErrorAction SilentlyContinue
        return $false
    }
    & pwsh -NoProfile -File $installer
    $code = $LASTEXITCODE
    Remove-Item $installer -ErrorAction SilentlyContinue
    return ($code -eq 0)
}

if (Test-Tool pengj-templates-cli) {
    Report-Ok "pengj-templates-cli $((& pengj-templates-cli --version 2>$null | Select-Object -First 1))"
} elseif ($InstallCli) {
    if (Install-CliTool) {
        if (Test-Tool pengj-templates-cli) {
            Report-Ok "pengj-templates-cli $((& pengj-templates-cli --version 2>$null | Select-Object -First 1))"
        } else {
            Report-Warn "pengj-templates-cli is installed but this shell's PATH is stale; reopen the terminal and run doctor again"
        }
    } else {
        Report-Fail "could not install pengj-templates-cli automatically; install manually: $CliHint"
    }
} else {
    Report-Fail "pengj-templates-cli not found; the commit gate and the agent workspace links both need it. Install: $CliHint (or run this script with --install-cli)"
}



if (Test-Tool node) {
    Report-Ok "node $((& node --version 2>$null))"
} else {
    Report-Warn "node not found; it is required by the mcp-bootstrap skill (install Node.js or drop that skill from options.skills)"
}



# --- commit gate activation ------------------------------------------------

$inside = $false
if (Test-Tool git) {
    $inside = [bool](& git rev-parse --show-toplevel 2>$null)
}
if ($inside) {
    $local = (& git config --local --get core.hooksPath 2>$null)

    if (-not (Test-Path ".githooks/pre-commit") -or -not (Test-Path ".githooks/commit-msg")) {
        Report-Fail ".githooks is incomplete; expected pre-commit and commit-msg"
    } elseif ($local -eq ".githooks") {
        Report-Ok "commit hooks active (core.hooksPath=.githooks)"
    } elseif ($local) {
        Report-Warn "core.hooksPath is '$local', not .githooks; the commit gate is not active here"
    } elseif ($Fix) {
        # Activate through the zero-dependency installer shipped by the gate
        # layer, but judge the outcome by the resulting git state: an
        # informational line failing to print must not turn a successful
        # activation into an error.
        if ((Test-Path ".githooks/install.sh") -and (Test-Tool sh)) {
            & sh .githooks/install.sh | Out-Null
        }
        & git update-index --chmod=+x .githooks/pre-commit .githooks/commit-msg 2>$null | Out-Null
        if ((& git config --local --get core.hooksPath 2>$null) -eq ".githooks") {
            Report-Ok "commit hooks activated (core.hooksPath=.githooks)"
        } else {
            Report-Fail "could not activate the commit hooks; run: git config core.hooksPath .githooks"
        }
    } else {
        Report-Warn "commit hooks are not active; run 'pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 --fix' or 'git config core.hooksPath .githooks'"
    }
} else {
    Report-Warn "not a git work tree; skipping the commit hook check"
}



# --- agent client workspace links -----------------------------------------

if (Test-Path ".agents") {
    if (Test-Tool pengj-templates-cli) {
        & pengj-templates-cli repo status --dir . 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Report-Ok "workspace links ready (agent clients)"
        } elseif ($Fix) {
            & pengj-templates-cli repo init --dir . 2>$null | Out-Null
            $initCode = $LASTEXITCODE
            & pengj-templates-cli repo status --dir . 2>$null | Out-Null
            if ($initCode -eq 0 -and $LASTEXITCODE -eq 0) {
                Report-Ok "workspace links created (agent clients)"
            } else {
                Report-Fail "could not create the workspace links; run: pengj-templates-cli repo init"
            }
        } else {
            Report-Fail "workspace links are not ready; run 'just doctor --fix'"
        }
    } else {
        Report-Warn "pengj-templates-cli not found; the workspace links cannot be checked or created without it"
    }
}


# --- custom checks ---------------------------------------------------------

$custom = 0
$customDir = Join-Path $root ".pengj-templates/scripts/doctor.d"
if (Test-Path $customDir) {
    Get-ChildItem -Path $customDir -Filter "*.ps1" | Sort-Object Name | ForEach-Object {
        $custom++
        $name = $_.Name
        & pwsh -NoProfile -File $_.FullName
        if ($LASTEXITCODE -eq 0) {
            $script:checks++
            Write-Host "[doctor] custom ok    $name" -ForegroundColor Green
        } else {
            $script:checks++
            $script:errors++
            Write-Host "[doctor] custom error $name" -ForegroundColor Red
        }
    }
}
if ($custom -eq 0) {
    Write-Host "[doctor] custom: none (add *.ps1 scripts to .pengj-templates/scripts/doctor.d/ to extend this check)"
}

Write-Host "[doctor] $script:checks checks: $script:errors errors, $script:warnings warnings"
if ($script:errors -gt 0) {
    exit 1
}
exit 0