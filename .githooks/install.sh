#!/bin/sh
# gate install: activate the gate hooks on this clone.
#
# `core.hooksPath` lives in `.git/config`, which never comes with a clone, so every clone needs this once. The script needs git and a POSIX shell and nothing else — it does not need the CLI, because activating the hooks and running them are separate jobs.
#
# The gate itself lives in `pengj-templates-cli`; this script ships no rule set of its own. When the CLI is missing the hooks block commits and print the install command, so the check below only warns.

set -eu

root=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "[gate] this is not a git work tree" >&2
    exit 1
}
cd "$root"

if [ ! -f .githooks/commit-msg ]; then
    echo "[gate] this repository does not carry the gate hooks" >&2
    exit 1
fi

# git reads the hooks from `.git/hooks` by default, and `core.hooksPath` lives in `.git/config`, which is never cloned. Pointing it at the tracked `.githooks/` is what makes the gate run at all.
git config core.hooksPath .githooks

# On Unix git silently skips a hook that is not executable. The tracked mode bit is the source of truth, and `update-index` is what records it.
for hook in commit-msg pre-commit pre-push; do
    [ -f ".githooks/$hook" ] || continue
    if git ls-files --error-unmatch ".githooks/$hook" >/dev/null 2>&1; then
        git update-index --chmod=+x ".githooks/$hook"
    else
        chmod +x ".githooks/$hook" 2>/dev/null || true
        echo "[gate] .githooks/$hook is not tracked yet; the executable bit was set on disk only" >&2
    fi
done

if command -v pengj-templates-cli >/dev/null 2>&1; then
    echo "[gate] pengj-templates-cli is installed: the hooks will run the full rule set"
else
    echo "[gate] the hooks are active, but pengj-templates-cli is not installed: commits will be blocked until it is" >&2
    echo "[gate]   the gate lives in the CLI; install it once:" >&2
    echo "[gate]   irm https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.ps1 | iex" >&2
    echo "[gate]   curl -fsSL https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.sh | sh" >&2
fi

configured=$(git config --local --get core.hooksPath 2>/dev/null || true)
if [ "$configured" = ".githooks" ]; then
    echo "[gate] the local commit gate is enabled: core.hooksPath=.githooks" 2>/dev/null || true
else
    echo "[gate] could not write core.hooksPath; run: git config core.hooksPath .githooks" >&2
    exit 1
fi