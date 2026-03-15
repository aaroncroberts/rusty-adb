#!/usr/bin/env bash
# build.sh — build rusty-adb
#
# Usage:
#   ./scripts/build.sh            # release build (default)
#   ./scripts/build.sh --debug    # debug build
#   ./scripts/build.sh --help     # show this message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/check-deps.sh"

# ── Argument parsing ───────────────────────────────────────────────────────────
PROFILE="release"
CARGO_FLAGS=("--release")

for arg in "$@"; do
    case "$arg" in
        --debug)
            PROFILE="debug"
            CARGO_FLAGS=()
            ;;
        --help|-h)
            echo "Usage: $(basename "$0") [--debug] [--help]"
            echo ""
            echo "  --debug   Build a debug binary (faster compile, larger binary)"
            echo "  --help    Show this help and exit"
            exit 0
            ;;
        *)
            fail "Unknown argument: $arg"
            echo "Run '$(basename "$0") --help' for usage."
            exit 1
            ;;
    esac
done

# ── Run ────────────────────────────────────────────────────────────────────────
check_deps

header "Building rusty-adb (${PROFILE})…"
info "Workspace: ${REPO_ROOT}"

cd "$REPO_ROOT"
cargo build "${CARGO_FLAGS[@]}"

BINARY="$REPO_ROOT/target/${PROFILE}/rusty-adb"
if [ ! -f "$BINARY" ]; then
    fail "Build succeeded but binary not found at: ${BINARY}"
    exit 1
fi

BINARY_SIZE="$(du -sh "$BINARY" | cut -f1)"
echo ""
success "Build complete!"
info "Binary : ${BINARY}"
info "Size   : ${BINARY_SIZE}"
