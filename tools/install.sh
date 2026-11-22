#!/usr/bin/env sh
# tools/install.sh
#
# bque One-Liner Quick Installer & Updater for Linux
#
# Usage:
#   curl -fsSL https://github.com/pengjiaxusz/bque/releases/latest/download/install.sh | sh

set -eu

REPO="pengjiaxusz/bque"
BASE_URL="https://github.com/${REPO}/releases/latest/download"
DEFAULT_INSTALL_DIR="${BQUE_INSTALL_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/bque/bin}"

echo ""
echo "================================================================"
echo "          bque (Build Queue & Governor) Installer / Updater     "
echo "================================================================"
echo ""

OS="$(uname -s)"
ARCH="$(uname -m)"

if [ "$OS" != "Linux" ]; then
    echo "Error: install.sh currently supports Linux (detected: $OS)." >&2
    echo "You can install from source via: cargo install --git https://github.com/${REPO}.git" >&2
    exit 1
fi

case "$ARCH" in
    x86_64|amd64)
        ASSET_BIN="bque-linux-x86_64"
        ASSET_TAR="bque-linux-x86_64.tar.gz"
        ;;
    *)
        echo "Error: no prebuilt Linux binary for architecture '$ARCH'." >&2
        echo "Install from source via: cargo install --git https://github.com/${REPO}.git" >&2
        exit 1
        ;;
esac

OLD_VERSION=""
EXISTING_BIN="$(command -v bque 2>/dev/null || true)"
if [ -n "$EXISTING_BIN" ] && [ -x "$EXISTING_BIN" ]; then
    OLD_VERSION="$("$EXISTING_BIN" --version 2>/dev/null | sed -n 's/.*\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -n 1 || true)"
fi

download_file() {
    url="$1"
    dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fLs --connect-timeout 15 --max-time 60 -o "$dest" "$url" >/dev/null 2>&1
        return $?
    elif command -v wget >/dev/null 2>&1; then
        wget -q -T 30 -O "$dest" "$url" >/dev/null 2>&1
        return $?
    else
        echo "Error: neither 'curl' nor 'wget' is available." >&2
        return 1
    fi
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bque-install.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

TMP_BIN="${TMP_DIR}/bque"

echo "--> Downloading latest bque from GitHub Releases..."
DOWNLOAD_OK=0
if download_file "${BASE_URL}/${ASSET_BIN}" "$TMP_BIN" && [ -s "$TMP_BIN" ]; then
    DOWNLOAD_OK=1
else
    echo "--> Direct binary download failed, trying tar.gz archive..."
    TMP_TAR="${TMP_DIR}/${ASSET_TAR}"
    if download_file "${BASE_URL}/${ASSET_TAR}" "$TMP_TAR" && [ -s "$TMP_TAR" ]; then
        if tar -xzf "$TMP_TAR" -C "$TMP_DIR" bque 2>/dev/null && [ -s "$TMP_BIN" ]; then
            DOWNLOAD_OK=1
        fi
    fi
fi

if [ "$DOWNLOAD_OK" -ne 1 ]; then
    echo "Error: failed to acquire bque from GitHub Releases." >&2
    echo "Check your network connection or download manually from:" >&2
    echo "  https://github.com/${REPO}/releases/latest" >&2
    exit 1
fi

chmod 0755 "$TMP_BIN"
mkdir -p "$DEFAULT_INSTALL_DIR"

TARGET_BIN="${DEFAULT_INSTALL_DIR}/bque"
STAGED_BIN="${DEFAULT_INSTALL_DIR}/.bque.new.$$"
cp "$TMP_BIN" "$STAGED_BIN"
chmod 0755 "$STAGED_BIN"
mv -f "$STAGED_BIN" "$TARGET_BIN"

NEW_VER_OUTPUT="$("$TARGET_BIN" --version 2>/dev/null || echo "bque")"
echo ""
if [ -n "$OLD_VERSION" ]; then
    echo "bque updated successfully: v${OLD_VERSION} -> ${NEW_VER_OUTPUT}"
else
    echo "bque installed successfully!"
    echo "Version: ${NEW_VER_OUTPUT}"
fi
echo "Installed location:"
echo "  - ${TARGET_BIN}"

case ":${PATH:-}:" in
    *":${DEFAULT_INSTALL_DIR}:"*)
        echo ""
        echo "Ready to use in the current session."
        ;;
    *)
        echo ""
        echo "Note: ${DEFAULT_INSTALL_DIR} is not currently on your PATH."
        echo "Add it to your shell profile (~/.bashrc or ~/.zshrc):"
        echo "  export PATH=\"${DEFAULT_INSTALL_DIR}:\$PATH\""
        ;;
esac
