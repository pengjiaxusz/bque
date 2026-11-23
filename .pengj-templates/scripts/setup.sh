#!/bin/sh
# Managed by pengj-templates (common layer).
# Zero-prerequisite thin bootstrap wrapper for repository environment setup.
# Installs pengj-templates-cli if missing, then hands off to `pengj-templates-cli setup --dir .`.

set -eu

PENGJ_BIN="${XDG_DATA_HOME:-$HOME/.local/share}/pengj-templates/bin"
case ":$PATH:" in
  *":$PENGJ_BIN:"*) ;;
  *) PATH="$PATH:$PENGJ_BIN"; export PATH ;;
esac

if ! command -v pengj-templates-cli >/dev/null 2>&1; then
  DRY_RUN=0
  for arg in "$@"; do
    if [ "$arg" = "--dry-run" ]; then
      DRY_RUN=1
      break
    fi
  done

  if [ "$DRY_RUN" -eq 1 ]; then
    echo "[setup] [dry-run] pengj-templates-cli is not installed."
    echo "[setup] [dry-run] Would download and install pengj-templates-cli to $PENGJ_BIN, then run: pengj-templates-cli setup --dir ."
    exit 0
  fi

  echo "[setup] Installing pengj-templates-cli to $PENGJ_BIN..." >&2
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.sh | sh
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.sh | sh
  else
    echo "[setup] Error: neither curl nor wget is available to download pengj-templates-cli." >&2
    exit 1
  fi

  if [ -n "${GITHUB_PATH:-}" ]; then
    printf '%s\n' "$PENGJ_BIN" >> "$GITHUB_PATH"
  fi

  if ! command -v pengj-templates-cli >/dev/null 2>&1; then
    echo "[setup] Error: pengj-templates-cli was not found in PATH after installation ($PENGJ_BIN)." >&2
    exit 1
  fi
fi

exec pengj-templates-cli setup --dir . "$@"