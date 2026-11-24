#!/bin/sh
# Managed by pengj-templates (common layer).
# Non-interactive bootstrap wrapper for repository environment checks.
# Never downloads/installs tools automatically and never prompts for y/n input.

set -eu

PENGJ_BIN="${XDG_DATA_HOME:-$HOME/.local/share}/pengj-templates/bin"
LOCAL_BIN="$HOME/.local/bin"
case ":$PATH:" in
  *":$LOCAL_BIN:"*) ;;
  *) PATH="$LOCAL_BIN:$PATH"; export PATH ;;
esac
case ":$PATH:" in
  *":$PENGJ_BIN:"*) ;;
  *) PATH="$PENGJ_BIN:$PATH"; export PATH ;;
esac

if command -v just >/dev/null 2>&1 \
  && command -v pengj-templates-cli >/dev/null 2>&1 \
  && { [ -f justfile ] || [ -f Justfile ]; }; then
  exec just doctor "$@"
fi

if command -v pengj-templates-cli >/dev/null 2>&1; then
  exec pengj-templates-cli doctor --dir . "$@"
fi

echo "[doctor] Missing required local tool(s). Nothing was downloaded or modified." >&2
echo "" >&2
echo "  - pengj-templates-cli (~5-10 MB)" >&2
echo "    Default install path: $PENGJ_BIN" >&2
echo "    Install (Linux/macOS):" >&2
echo "      curl -fsSL https://github.com/pengjiaxusz/pengj-templates/releases/latest/download/install.sh | sh" >&2

if ! command -v just >/dev/null 2>&1; then
  echo "" >&2
  echo "  - just (>= 1.56.0, ~3-5 MB, optional if calling the CLI directly)" >&2
  echo "    Typical install path: ~/.local/bin or ~/.cargo/bin (avoid outdated apt packages)" >&2
  echo "    Install: curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | sh -s -- --to ~/.local/bin" >&2
  echo "    Or: brew install just / cargo install just / https://github.com/casey/just#installation" >&2
fi

echo "" >&2
echo "After installing, re-run:" >&2
echo "  sh .pengj-templates/scripts/doctor.sh" >&2
echo "  # or: just doctor (pass --fix explicitly to fix local hooks/links)" >&2
exit 1