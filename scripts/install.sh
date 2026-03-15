#!/usr/bin/env bash
# install.sh — build and install the rusty-adb release binary
#
# Install locations (in priority order):
#   1. ~/bin           (if it exists and is in PATH)
#   2. /usr/local/bin  (default fallback; may require sudo)
#
# Usage:
#   ./scripts/install.sh                     # build + install
#   ./scripts/install.sh --prefix ~/bin      # install to a custom path
#   ./scripts/install.sh --help              # show this message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/check-deps.sh"

# ── Argument parsing ───────────────────────────────────────────────────────────
PREFIX=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            PREFIX="${2:?'--prefix requires a path argument'}"
            shift 2
            ;;
        --prefix=*)
            PREFIX="${1#*=}"
            shift
            ;;
        --help|-h)
            echo "Usage: $(basename "$0") [--prefix <dir>] [--help]"
            echo ""
            echo "  --prefix <dir>   Install binary into <dir> instead of auto-detected location"
            echo "  --help           Show this help and exit"
            echo ""
            echo "Default install locations (first match wins):"
            echo "  ~/bin            if it exists and is in \$PATH"
            echo "  /usr/local/bin   (may require sudo)"
            exit 0
            ;;
        *)
            fail "Unknown argument: $1"
            echo "Run '$(basename "$0") --help' for usage."
            exit 1
            ;;
    esac
done

# ── Resolve install directory ──────────────────────────────────────────────────
_resolve_install_dir() {
    if [ -n "$PREFIX" ]; then
        echo "$PREFIX"
        return
    fi
    # Prefer ~/bin when it exists and is already in PATH
    local user_bin="$HOME/bin"
    if [ -d "$user_bin" ] && [[ ":$PATH:" == *":$user_bin:"* ]]; then
        echo "$user_bin"
        return
    fi
    echo "/usr/local/bin"
}

INSTALL_DIR="$(_resolve_install_dir)"

# ── Run ────────────────────────────────────────────────────────────────────────
check_deps

header "Building rusty-adb (release)…"
cd "$REPO_ROOT"
cargo build --release

BINARY="$REPO_ROOT/target/release/rusty-adb"
if [ ! -f "$BINARY" ]; then
    fail "Release binary not found at: ${BINARY}"
    exit 1
fi

header "Installing to ${INSTALL_DIR}…"

# Create destination directory if it doesn't exist
if [ ! -d "$INSTALL_DIR" ]; then
    info "Creating directory: ${INSTALL_DIR}"
    mkdir -p "$INSTALL_DIR"
fi

DEST="$INSTALL_DIR/rusty-adb"

# Use sudo only for /usr/local/bin when not writable by current user
if [ ! -w "$INSTALL_DIR" ]; then
    info "Directory not writable — using sudo."
    sudo cp "$BINARY" "$DEST"
    sudo chmod +x "$DEST"
else
    cp "$BINARY" "$DEST"
    chmod +x "$DEST"
fi

BINARY_SIZE="$(du -sh "$DEST" | cut -f1)"
echo ""
success "Installed!"
info "Location : ${DEST}"
info "Size     : ${BINARY_SIZE}"

# ── PATH hint ──────────────────────────────────────────────────────────────────
if ! command -v rusty-adb &>/dev/null; then
    echo ""
    warn "rusty-adb is not yet on your \$PATH."
    if [[ "$INSTALL_DIR" == "$HOME/bin" ]]; then
        echo  "      Add this to your shell profile (~/.zshrc or ~/.bashrc):"
        echo  "        export PATH=\"\$HOME/bin:\$PATH\""
    else
        echo  "      Make sure ${INSTALL_DIR} is in your \$PATH."
        echo  "      Add to your shell profile (~/.zshrc or ~/.bashrc):"
        echo  "        export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
else
    info "Run  : rusty-adb"
fi
