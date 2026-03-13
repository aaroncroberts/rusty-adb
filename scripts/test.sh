#!/usr/bin/env bash
# test.sh — full quality gate for rusty-adb
#
# Runs in order:
#   1. cargo test --all
#   2. cargo clippy --all-targets -- -D warnings
#   3. cargo fmt --all --check
#
# Any failure stops the gate and exits non-zero.
#
# Usage:
#   ./scripts/test.sh           # run full quality gate
#   ./scripts/test.sh --help    # show this message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/check-deps.sh"

# ── Argument parsing ───────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --help|-h)
            echo "Usage: $(basename "$0") [--help]"
            echo ""
            echo "Runs the full quality gate:"
            echo "  1. cargo test --all"
            echo "  2. cargo clippy --all-targets -- -D warnings"
            echo "  3. cargo fmt --all --check"
            exit 0
            ;;
        *)
            fail "Unknown argument: $arg"
            echo "Run '$(basename "$0") --help' for usage."
            exit 1
            ;;
    esac
done

# ── Helpers ────────────────────────────────────────────────────────────────────
PASS_COUNT=0
FAIL_COUNT=0
FAILED_STEPS=()

run_step() {
    local label="$1"
    shift
    info "Running: $*"
    if "$@"; then
        success "${label}"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        fail "${label}"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        FAILED_STEPS+=("$label")
        return 1
    fi
}

# ── Run ────────────────────────────────────────────────────────────────────────
check_deps

cd "$REPO_ROOT"

header "Quality gate"

echo ""
echo -e "${BOLD}Step 1/3 — Tests${RESET}"
run_step "Tests passed" cargo test --all

echo ""
echo -e "${BOLD}Step 2/3 — Lint (clippy)${RESET}"
run_step "Clippy clean" cargo clippy --all-targets -- -D warnings

echo ""
echo -e "${BOLD}Step 3/3 — Formatting${RESET}"
run_step "Formatting clean" cargo fmt --all --check

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}─────────────────────────────────────${RESET}"
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo -e "${GREEN}${BOLD}  ✓ All ${PASS_COUNT} checks passed${RESET}"
    exit 0
else
    echo -e "${RED}${BOLD}  ✗ ${FAIL_COUNT} check(s) failed:${RESET}"
    for step in "${FAILED_STEPS[@]}"; do
        echo -e "${RED}      • ${step}${RESET}"
    done
    exit 1
fi
