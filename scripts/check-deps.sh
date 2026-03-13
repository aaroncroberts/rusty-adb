#!/usr/bin/env bash
# check-deps.sh — shared prerequisite checker for rusty-adb scripts
#
# Source this file from other scripts; do not run it directly.
# It exports:
#   REPO_ROOT  — absolute path to the workspace root
#   ADB_PATH   — path to the adb binary (may be empty if not required)
#
# Usage in other scripts:
#   SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#   source "$SCRIPT_DIR/check-deps.sh"

# ── Colours ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# Disable colour when not writing to a terminal (e.g. CI log files)
if [ ! -t 1 ]; then
    RED='' GREEN='' YELLOW='' CYAN='' BOLD='' RESET=''
fi

# ── Helpers ────────────────────────────────────────────────────────────────────
info()    { echo -e "${CYAN}  →${RESET} $*"; }
success() { echo -e "${GREEN}  ✓${RESET} $*"; }
warn()    { echo -e "${YELLOW}  ⚠${RESET} $*"; }
fail()    { echo -e "${RED}  ✗${RESET} $*" >&2; }
header()  { echo -e "\n${BOLD}$*${RESET}"; }

# ── Resolve REPO_ROOT ──────────────────────────────────────────────────────────
# Always the directory that contains this scripts/ folder
_DEPS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$_DEPS_DIR/.." && pwd)"
export REPO_ROOT

# ── Check: Rust / cargo ────────────────────────────────────────────────────────
_check_rust() {
    if ! command -v cargo &>/dev/null; then
        fail "cargo not found."
        echo    "      Install Rust: https://rustup.rs"
        echo    "        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        return 1
    fi
    local rust_ver
    rust_ver="$(rustc --version 2>/dev/null | awk '{print $2}')"
    success "Rust ${rust_ver} ($(command -v cargo))"
}

# ── Check: adb (optional) ──────────────────────────────────────────────────────
# Sets ADB_PATH. Prints a warning (not an error) when adb is absent, because
# some scripts (build, test) don't need adb at all.
_check_adb() {
    ADB_PATH=""
    # 1. Check PATH
    if command -v adb &>/dev/null; then
        ADB_PATH="$(command -v adb)"
        success "adb found: ${ADB_PATH}"
        return 0
    fi
    # 2. Android SDK default location on macOS
    local sdk_adb="$HOME/Library/Android/sdk/platform-tools/adb"
    if [ -x "$sdk_adb" ]; then
        ADB_PATH="$sdk_adb"
        success "adb found: ${ADB_PATH}"
        return 0
    fi
    warn "adb not found (optional for build/test; required to run the app)."
    echo  "      macOS: brew install --cask android-platform-tools"
    echo  "      Or download: https://developer.android.com/tools/releases/platform-tools"
    export ADB_PATH
}

# ── Public entry point ─────────────────────────────────────────────────────────
# Call check_deps [--require-adb] to run all checks.
# Exits 1 if any required dependency is missing.
check_deps() {
    local require_adb=false
    for arg in "$@"; do
        [[ "$arg" == "--require-adb" ]] && require_adb=true
    done

    header "Checking dependencies…"

    local ok=true

    _check_rust    || ok=false
    _check_adb

    if $require_adb && [ -z "$ADB_PATH" ]; then
        fail "adb is required but was not found (see above)."
        ok=false
    fi

    export ADB_PATH

    if ! $ok; then
        echo ""
        fail "One or more required dependencies are missing. Aborting."
        exit 1
    fi

    echo ""
}
