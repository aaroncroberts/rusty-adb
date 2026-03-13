#!/usr/bin/env bash
# dev.sh — run rusty-adb in development mode
#
# Uses cargo-watch when available for auto-rebuild on file changes,
# otherwise falls back to a plain cargo run.
#
# Usage:
#   ./scripts/dev.sh            # run with auto-reload if cargo-watch available
#   ./scripts/dev.sh --no-watch # plain cargo run, no file watching
#   ./scripts/dev.sh --help     # show this message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/check-deps.sh"

# ── Argument parsing ───────────────────────────────────────────────────────────
NO_WATCH=false

for arg in "$@"; do
    case "$arg" in
        --no-watch)
            NO_WATCH=true
            ;;
        --help|-h)
            echo "Usage: $(basename "$0") [--no-watch] [--help]"
            echo ""
            echo "  --no-watch   Skip cargo-watch; run once with cargo run"
            echo "  --help       Show this help and exit"
            echo ""
            echo "Install cargo-watch for auto-reload on file changes:"
            echo "  cargo install cargo-watch"
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

header "Starting rusty-adb in development mode…"
info "Workspace: ${REPO_ROOT}"

if ! $NO_WATCH && command -v cargo-watch &>/dev/null; then
    info "cargo-watch detected — will rebuild on file changes."
    info "Press Ctrl-C to stop."
    echo ""
    exec cargo watch -x "run --package rusty-adb"
else
    if ! $NO_WATCH; then
        warn "cargo-watch not found — running without auto-reload."
        echo  "      Install with: cargo install cargo-watch"
        echo ""
    fi
    info "Press Ctrl-C to stop."
    echo ""
    exec cargo run --package rusty-adb
fi
