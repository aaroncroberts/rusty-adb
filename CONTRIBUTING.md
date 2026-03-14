# Contributing to rusty-adb

```text
╔════════════════════════════════════════╗
║   CONTRIBUTING TO rusty-adb           ║
║   ─── phosphor green all the way ───  ║
╚════════════════════════════════════════╝
```

Thanks for your interest in contributing. This document covers everything you need to go from a fresh clone to an open pull request.

---

## Table of Contents

- [Dev Environment Setup](#dev-environment-setup)
- [Cargo Commands](#cargo-commands)
- [Testing Strategy](#testing-strategy)
- [Code Style](#code-style)
- [PR Checklist](#pr-checklist)
- [Project Conventions](#project-conventions)

---

## Dev Environment Setup

### 1. Prerequisites

- **Rust stable** (1.75+) — install via [rustup](https://rustup.rs)
- **adb** — see the [README prerequisites](README.md#prerequisites) for platform-specific install instructions

### 2. Clone and build

```bash
git clone https://github.com/aaroncroberts/rusty-adb.git
cd rusty-adb
cargo build
```

### 3. Run the app

```bash
cargo run --package rusty-adb
# or with the dev script (uses cargo-watch for auto-rebuild):
./scripts/dev.sh
```

### 4. Optional — cargo-watch (auto-reload on file changes)

```bash
cargo install cargo-watch
./scripts/dev.sh   # now auto-rebuilds on every save
```

---

## Cargo Commands

| Command | Purpose |
|---|---|
| `cargo build` | Debug build |
| `cargo build --release` | Release build |
| `cargo run --package rusty-adb` | Run the app |
| `cargo test --all` | Run all unit + integration tests |
| `cargo clippy --all-targets -- -D warnings` | Lint (zero warnings enforced) |
| `cargo fmt --all` | Auto-format all source files |
| `cargo fmt --all --check` | Check formatting without modifying files |
| `cargo doc --no-deps --open` | Build and open rustdoc |

The **scripts/** directory wraps the most common workflows:

```bash
./scripts/test.sh    # cargo test + clippy + fmt --check in one step
./scripts/build.sh   # release build with binary size output
```

---

## Testing Strategy

rusty-adb uses three layers of tests.

### Unit tests (`src/**/*.rs`)

Logic that can be tested without a real device or process lives in `#[cfg(test)]` modules inside each source file. These cover:

- `adb/parser.rs` — `parse_ls_output`, `DeviceState::from_str`, display formatting
- `adb/transfer.rs` — `parse_progress_line`, `parse_speed`, the `run_transfer_emits_failed_on_bad_exit` path
- `adb/mod.rs` — `AdbStatus::text()` output for every variant
- `config.rs` — YAML parsing, default fallbacks
- `fs/mod.rs` — `SortField` ordering, `DirEntry` formatting

Run with: `cargo test --all`

### Integration tests (`tests/adb_integration.rs`)

These use a **mock-adb** bash script (`tests/fixtures/mock-adb`) instead of a real `adb` binary. The mock script implements the minimum subset of `adb` needed to test `AdbClient`:

- `devices -l` — returns a fake authorised device
- `shell ls -la` — returns synthetic directory listings
- `push`/`pull --progress` — emits fake progress lines on stderr
- `shell mv`, `shell rm -rf`, `disconnect`, `start-server`

`AdbClient` is constructed with an explicit `adb_path` pointing at the mock, so no real Android device or adb daemon is needed. Tests run fully offline.

**Adding a new integration test:**

1. Add a `#[tokio::test]` function in `tests/adb_integration.rs`.
2. Use `make_client()` (already defined in that file) to get an `AdbClient` backed by `mock-adb`.
3. If the mock needs to handle a new command, add a `case` branch in `tests/fixtures/mock-adb`.

### Transfer failure test

`adb/transfer.rs` includes `run_transfer_emits_failed_on_bad_exit` (`#[cfg(unix)]`) which creates a temporary `exit 1` shell script and verifies that `run_transfer` emits `TransferEvent::Failed` — without needing a real process.

---

## Code Style

- **Formatting**: `rustfmt` with default settings. Run `cargo fmt --all` before committing.
- **Lints**: `cargo clippy --all-targets -- -D warnings` must pass with zero warnings. Fix all warnings before opening a PR — the CI pipeline enforces this.
- **Error handling**: Use `anyhow::Result` for fallible functions. Include context (serial, path) in error strings so failures are debuggable in logs.
- **No hot-path `unwrap()`**: Use `.expect("reason")` with a short explanation, or `?` propagation.
- **DRY**: Extract shared patterns into private helpers (see `check_adb_output()` in `adb/mod.rs` and `modal_backdrop()` in `views/modals.rs`).
- **Iced widgets**: Keep `view_*` methods focused — build the widget tree, return `Element`. Business logic belongs in `update()`.

---

## PR Checklist

Before opening a pull request, verify:

- [ ] `cargo test --all` passes
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --all --check` reports no differences
- [ ] New public types and functions have `///` doc comments
- [ ] New behaviour is covered by at least one unit or integration test
- [ ] Commit messages are descriptive (`feat:`, `fix:`, `chore:`, `docs:` prefixes)

Or run the one-shot quality gate:

```bash
./scripts/test.sh
```

---

## Project Conventions

### Commit message format

```
<type>: <short summary>

[optional body]

Co-Authored-By: ...
```

Types: `feat` · `fix` · `refactor` · `test` · `docs` · `chore` · `style`

### File organisation

Source is organised into focused module directories under `src/`. Each directory has a `mod.rs` that owns the public API and delegates to sibling files for large or distinct concerns:

- `src/adb/` — ADB client, domain types (`AdbStatus`, `TransferStatus`), and the transfer engine
- `src/fs/` — `FileSystem` trait, `DirEntry`, and both filesystem backends (local + Android)
- `src/file_pane/` — generic `FilePane<FS>` pane widget and its view helpers
- `src/views/` — all Iced widget rendering (status bar, modals, banners, pane controls)
- `src/update/` — all `App::update()` handlers, one focused `impl App` block per file
- `src/config.rs` and `src/theme.rs` — single-file modules (small enough to not need a directory)

Integration tests live in `tests/` at the crate root. Developer scripts live in `scripts/` at the workspace root. Architecture and design docs live in `docs/`.

**Dependency rule:** `adb/` and `fs/` are the base layer — they must not import from `views/`, `update/`, or `file_pane/`. If you find yourself wanting to make that import, the type belongs in `adb/` or `fs/` instead.

### Adding a new feature

1. Add `Message` variants for any new user-driven events.
2. Implement the logic in `App::update()`.
3. Wire the UI in the relevant `view_*` method.
4. Write unit tests in the affected module's `#[cfg(test)]` block.
5. If the feature touches `AdbClient`, add an integration test in `tests/adb_integration.rs`.
