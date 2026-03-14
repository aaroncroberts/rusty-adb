#!/usr/bin/env bash
# dev.sh — run rusty-adb in development mode
#
# Usage:
#   ./scripts/dev.sh         # build and run
#   ./scripts/dev.sh --help  # show this message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/check-deps.sh"

# ── Argument parsing ───────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --help|-h)
            echo "Usage: $(basename "$0") [--help]"
            echo ""
            echo "  Builds and runs rusty-adb. Press Ctrl-C to stop."
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

cd "$REPO_ROOT"

header "Starting rusty-adb…"
info "Workspace: ${REPO_ROOT}"
info "Press Ctrl-C to stop."
echo ""

exec cargo run --package rusty-adb
