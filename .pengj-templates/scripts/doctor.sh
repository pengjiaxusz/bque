#!/bin/sh
# .pengj-templates/scripts/doctor.sh - project environment check.
#
# Checks only the tools the selected template layers actually use, then runs
# every custom check in .pengj-templates/scripts/doctor.d/*.sh in sorted order.
#
# Usage:
#   sh .pengj-templates/scripts/doctor.sh                 report only
#   sh .pengj-templates/scripts/doctor.sh --fix           also activate the commit hooks and fix modes
#   sh .pengj-templates/scripts/doctor.sh --install-cli   download and install pengj-templates-cli
#                                        (the only step that touches the network;
#                                        it is never implied by --fix)
#
# Custom checks: drop *.sh files into .pengj-templates/scripts/doctor.d/. Each script runs from
# the project root with DOCTOR_ROOT set to the project root and DOCTOR_FIX=1
# when --fix is given. Exit 0 = pass, non-zero = failure; output is shown as-is.

set -u

fix=0
install_cli=0
for arg in "$@"; do
    case "$arg" in
        --fix) fix=1 ;;
        --install-cli) install_cli=1 ;;
        -h|--help)
            echo "usage: sh .pengj-templates/scripts/doctor.sh [--fix] [--install-cli]"
            exit 0
            ;;
        *)
            echo "[doctor] unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
cd "$root" || exit 1
export DOCTOR_ROOT="$root"
if [ "$fix" = "1" ]; then
    export DOCTOR_FIX=1
fi

checks=0
errors=0
warnings=0

ok()   { checks=$((checks + 1)); printf '[doctor] ok    %s\n' "$1"; }
warn() { checks=$((checks + 1)); warnings=$((warnings + 1)); printf '[doctor] warn  %s\n' "$1"; }
fail() { checks=$((checks + 1)); errors=$((errors + 1)); printf '[doctor] error %s\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }
is_windows() {
    case "$(uname -s 2>/dev/null || echo unknown)" in
        MINGW*|MSYS*|CYGWIN*) return 0 ;;
        *) return 1 ;;
    esac
}

# --- built-in checks: only what the selected layers use ---------------------

if have git; then
    ok "git $(git --version | sed 's/^git version //')"
else
    fail "git not found"
fi


if have just; then
    ok "just $(just --version | sed 's/^just //')"
else
    fail "just not found; install: https://github.com/casey/just#installation"
fi
if is_windows; then
    if have pwsh; then
        ok "pwsh $(pwsh --version)"
    else
        fail "pwsh not found; the just recipes use PowerShell 7 on Windows"
    fi
fi



if have cargo; then
    ok "cargo $(cargo --version | sed 's/^cargo //')"
else
    fail "cargo not found; install a Rust toolchain"
fi
if have rustc; then
    ok "rustc $(rustc --version | sed 's/^rustc //')"
else
    fail "rustc not found; install a Rust toolchain"
fi



if is_windows; then
    if have bque; then
        ok "bque $(bque --version 2>/dev/null | head -n 1)"
    else
        warn "bque not found; install: irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex"
    fi
else
    ok "bque not required on this platform"
fi



# --- pengj-templates-cli ---------------------------------------------------
# The commit gate needs it, and the agent-layer workspace links are created by
# its `repo init`, so both layers require it: missing is an error, not a warning.
# Installing is the only network access and needs the explicit --install-cli;
# `--fix` never touches the network.
cli_release_base='https://github.com/pengjiaxusz/pengj-templates/releases/latest/download'
if is_windows; then
    cli_hint="irm $cli_release_base/install.ps1 | iex"
else
    cli_hint="curl -fsSL $cli_release_base/install.sh | sh"
fi
install_cli_tool() {
    installer="${TMPDIR:-/tmp}/pengj-install-$$.sh"
    if is_windows; then
        have pwsh || return 1
        pwsh -NoProfile -Command "irm $cli_release_base/install.ps1 | iex"
        return $?
    fi
    if have curl; then
        curl -fLs --connect-timeout 15 --max-time 120 -o "$installer" "$cli_release_base/install.sh" || return 1
    elif have wget; then
        wget -q --timeout=30 -O "$installer" "$cli_release_base/install.sh" || return 1
    else
        return 1
    fi
    sh "$installer"
    installer_status=$?
    rm -f "$installer"
    return $installer_status
}

if have pengj-templates-cli; then
    ok "pengj-templates-cli $(pengj-templates-cli --version 2>/dev/null | head -n 1)"
elif [ "$install_cli" = "1" ]; then
    if install_cli_tool; then
        if have pengj-templates-cli; then
            ok "pengj-templates-cli $(pengj-templates-cli --version 2>/dev/null | head -n 1)"
        else
            warn "pengj-templates-cli is installed but this shell's PATH is stale; reopen the terminal and run doctor again"
        fi
    else
        fail "could not install pengj-templates-cli automatically; install manually: {{ cli_hint }}"
    fi
else
    fail "pengj-templates-cli not found; the commit gate and the agent workspace links both need it. Install: {{ cli_hint }} (or run this script with --install-cli)"
fi



if have node; then
    ok "node $(node --version 2>/dev/null)"
else
    warn "node not found; it is required by the mcp-bootstrap skill (install Node.js or drop that skill from options.skills)"
fi



# --- commit gate activation ------------------------------------------------

if have git && git rev-parse --show-toplevel >/dev/null 2>&1; then
    local_hooks=$(git config --local --get core.hooksPath 2>/dev/null || true)

    if [ ! -f .githooks/pre-commit ] || [ ! -f .githooks/commit-msg ]; then
        fail ".githooks is incomplete; expected pre-commit and commit-msg"
    elif [ "$local_hooks" = ".githooks" ]; then
        ok "commit hooks active (core.hooksPath=.githooks)"
    elif [ -n "$local_hooks" ]; then
        warn "core.hooksPath is '$local_hooks', not .githooks; the commit gate is not active here"
    elif [ "$fix" = "1" ]; then
        # Activate through the zero-dependency installer shipped by the gate
        # layer, but judge the outcome by the resulting git state: an
        # informational line failing to print must not turn a successful
        # activation into an error.
        if [ -f .githooks/install.sh ]; then
            sh .githooks/install.sh >/dev/null || true
        fi
        # Record the mode bit when the hooks are already tracked, so clones on
        # Unix do not silently skip them.
        git update-index --chmod=+x .githooks/pre-commit .githooks/commit-msg 2>/dev/null || true
        if [ "$(git config --local --get core.hooksPath 2>/dev/null || true)" = ".githooks" ]; then
            ok "commit hooks activated (core.hooksPath=.githooks)"
        else
            fail "could not activate the commit hooks; run: git config core.hooksPath .githooks"
        fi
    else
        warn "commit hooks are not active; run 'sh .pengj-templates/scripts/doctor.sh --fix' or 'git config core.hooksPath .githooks'"
    fi

    # git silently skips a hook without the executable bit on Unix
    if ! is_windows; then
        for hook in pre-commit commit-msg; do
            if [ -f ".githooks/$hook" ] && [ ! -x ".githooks/$hook" ]; then
                if [ "$fix" = "1" ]; then
                    chmod +x ".githooks/$hook" && ok ".githooks/$hook is executable"
                else
                    warn ".githooks/$hook is not executable; run with --fix"
                fi
            fi
        done
    fi
else
    warn "not a git work tree; skipping the commit hook check"
fi



# --- agent client workspace links -----------------------------------------

if [ -d .agents ]; then
    if have pengj-templates-cli; then
        if pengj-templates-cli repo status --dir . >/dev/null 2>&1; then
            ok "workspace links ready (agent clients)"
        elif [ "$fix" = "1" ] && pengj-templates-cli repo init --dir . >/dev/null 2>&1 \
            && pengj-templates-cli repo status --dir . >/dev/null 2>&1; then
            ok "workspace links created (agent clients)"
        elif [ "$fix" = "1" ]; then
            fail "could not create the workspace links; run: pengj-templates-cli repo init"
        else
            fail "workspace links are not ready; run 'just doctor --fix'"
        fi
    else
        warn "pengj-templates-cli not found; the workspace links cannot be checked or created without it"
    fi
fi


# --- custom checks ---------------------------------------------------------

custom=0
if [ -d .pengj-templates/scripts/doctor.d ]; then
    for script in .pengj-templates/scripts/doctor.d/*.sh; do
        [ -f "$script" ] || continue
        custom=$((custom + 1))
        name=$(basename "$script")
        if sh "$script"; then
            checks=$((checks + 1))
            printf '[doctor] custom ok    %s\n' "$name"
        else
            checks=$((checks + 1))
            errors=$((errors + 1))
            printf '[doctor] custom error %s\n' "$name"
        fi
    done
fi
if [ "$custom" -eq 0 ]; then
    printf '[doctor] custom: none (add *.sh scripts to .pengj-templates/scripts/doctor.d/ to extend this check)\n'
fi

printf '[doctor] %d checks: %d errors, %d warnings\n' "$checks" "$errors" "$warnings"
if [ "$errors" -gt 0 ]; then
    exit 1
fi
exit 0