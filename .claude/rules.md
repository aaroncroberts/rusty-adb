# rusty-adb Agent Rules

## Branch Management

### Working Branch
- **ALWAYS work on `aaron/agentic-coder` branch**
- Never work directly on `main` branch
- Never delete the `aaron/agentic-coder` branch after merge

### Branch Protection
```bash
# Verify you're on the correct branch before any work
git branch --show-current  # Must show: aaron/agentic-coder
```

---

## Scripts — Use Them, Don't Bypass Them

The `scripts/` directory exists so all build/test/run actions are consistent. **Always prefer a script over a raw cargo command.**

| Task | Use this |
|------|----------|
| Run the app (dev loop) | `./scripts/dev.sh` |
| Run once without watch | `./scripts/dev.sh --no-watch` |
| Full quality gate | `./scripts/test.sh` |
| Release build | `./scripts/build.sh` |
| Debug build | `./scripts/build.sh --debug` |
| Install binary | `./scripts/install.sh` |

### cargo-watch is required
`cargo-watch` must be installed (`cargo install cargo-watch`). `dev.sh` uses it automatically. Without it, every code change requires a manual restart.

---

## Pre-Commit Discipline

### Required Checks Before ANY Commit

Run the one-shot quality gate:

```bash
./scripts/test.sh
```

This runs in order:
1. `cargo test --all` — all unit + integration tests
2. `cargo clippy --all-targets -- -D warnings` — zero warnings
3. `cargo fmt --all --check` — formatting clean

**DO NOT commit if any step fails.**

### Pre-Commit Checklist

```
[ ] 1. ./scripts/test.sh passes (all 3 steps)
[ ] 2. Beads synced (bd sync)
[ ] 3. Commit message written (conventional commits format)
[ ] 4. Relevant files staged (not git add -A blindly)
[ ] 5. Commit created
[ ] 6. Pushed to remote
```

---

## Commit Message Strategy

Use conventional commits format with a heredoc:

```bash
git commit -m "$(cat <<'EOF'
feat: implement expandable pane layout

- Add PaneLayout enum (Split/LocalExpanded/AndroidExpanded)
- Narrow 46px sidebar with vertical label and expand buttons
- Tooltips on >> and >< controls

Closes rusty-adb-xyz
EOF
)"
```

**Types:** `feat` · `fix` · `refactor` · `test` · `docs` · `chore` · `style`

---

## Session Close Protocol

**CRITICAL**: Before ending any session, complete this checklist:

```bash
# 1. Run quality gate
./scripts/test.sh

# 2. Check what changed
git status
git diff

# 3. Stage relevant files
git add <specific files>

# 4. Commit
git commit -m "$(cat <<'EOF'
<type>: <summary>
EOF
)"

# 5. Sync beads
bd sync

# 6. Push
git push origin aaron/agentic-coder
```

**NEVER** say "done" or "complete" without pushing to remote.

---

## Error Recovery

### If Build Fails
1. Read the error carefully
2. Fix the Rust compile error
3. Re-run `cargo build` (or let cargo-watch handle it)
4. DO NOT commit until build succeeds

### If Tests Fail
1. Investigate the failing test
2. Fix the issue (code or test)
3. Re-run `./scripts/test.sh`
4. DO NOT commit until tests pass

### If Clippy Fails
1. Read the warning/error
2. Fix the code (or add `#[allow(...)]` with justification)
3. Re-run `cargo clippy --all-targets -- -D warnings`
4. DO NOT commit until clean

---

## Common Commands Reference

```bash
# Dev loop (auto-rebuild on save)
./scripts/dev.sh

# Full quality gate
./scripts/test.sh

# Run tests only
cargo test --all

# Check compile (fast)
cargo check --all

# Beads workflow
bd ready                               # Find unblocked work
bd update <id> --status=in_progress   # Claim task
bd close <id> --reason="Completed"    # Close task
bd sync                                # Persist to remote

# Git workflow
git branch --show-current             # Verify branch
git status                             # Check state
git push origin aaron/agentic-coder   # Push work
```

---

## Key Principles

1. **Use the scripts** — consistency over convenience
2. **Quality First** — never commit broken code
3. **Test Everything** — all code changes must have tests
4. **Branch Discipline** — always on `aaron/agentic-coder`
5. **Authorization Required** — get approval before releases or force-pushes
6. **Document Changes** — update docs in the same PR as structural changes
7. **cargo-watch is your friend** — it eliminates the manual kill/rebuild/rerun loop
