#!/bin/sh
# Managed by pengj-templates (common layer).
# Zero-prerequisite bootstrap wrapper for project setup: installs or upgrades pengj-templates-cli if missing/outdated, then delegates to `pengj-templates-cli setup --dir .`.

set -eu

PENGJ_TMP="${XDG_CACHE_HOME:-$HOME/.cache}/pengj-templates/tmp"
mkdir -p "$PENGJ_TMP"
TMPDIR="$PENGJ_TMP"
export TMPDIR

PENGJ_BIN="${XDG_DATA_HOME:-$HOME/.local/share}/pengj-templates/bin"
case ":$PATH:" in
  *":$PENGJ_BIN:"*) ;;
  *) PATH="$PATH:$PENGJ_BIN"; export PATH ;;
esac

NEED_INSTALL=0
if ! command -v pengj-templates-cli >/dev/null 2>&1; then
  NEED_INSTALL=1
elif ! pengj-templates-cli setup --help >/dev/null 2>&1; then
  NEED_INSTALL=2
fi

if [ "$NEED_INSTALL" -ne 0 ]; then
  DRY_RUN=0
  for arg in "$@"; do
    if [ "$arg" = "--dry-run" ]; then
      DRY_RUN=1
      break
    fi
  done

  if [ "$DRY_RUN" -eq 1 ]; then
if [ "$NEED_INSTALL" -eq 2 ]; then
      echo "[setup] [dry-run] Existing pengj-templates-cli does not support the setup subcommand."
      echo "[setup] [dry-run] Would upgrade pengj-templates-cli (target: $PENGJ_BIN) and run: pengj-templates-cli setup --dir ."
    else
      echo "[setup] [dry-run] pengj-templates-cli not found."
      echo "[setup] [dry-run] Would download and install pengj-templates-cli (target: $PENGJ_BIN) and run: pengj-templates-cli setup --dir ."
    fi
exit 0
  fi

if [ "$NEED_INSTALL" -eq 2 ]; then
    echo "[setup] Upgrading pengj-templates-cli in $PENGJ_BIN..." >&2
  else
    echo "[setup] Installing pengj-templates-cli to $PENGJ_BIN..." >&2
  fi
if command -v curl >/dev/null 2>&1; then
    curl -fsSL https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.sh | sh
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.sh | sh
  else
echo "[setup] Error: neither curl nor wget is available to download pengj-templates-cli." >&2
exit 1
  fi

  if ! pengj-templates-cli setup --help >/dev/null 2>&1; then
echo "[setup] Error: pengj-templates-cli (with setup support) still unavailable in PATH after install (expected: $PENGJ_BIN)." >&2
exit 1
  fi
fi

if [ -n "${GITHUB_PATH:-}" ] && [ -d "$PENGJ_BIN" ]; then
  if [ ! -f "$GITHUB_PATH" ] || ! grep -Fxq "$PENGJ_BIN" "$GITHUB_PATH" 2>/dev/null; then
    printf '%s\n' "$PENGJ_BIN" >> "$GITHUB_PATH"
  fi
fi

exec pengj-templates-cli setup --dir . "$@"