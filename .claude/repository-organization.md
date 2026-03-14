# Repository Organisation — rusty-adb

Rules and conventions for agents working in this repository.

---

## Workspace Layout

```text
rusty-adb/                     # Cargo workspace root
├── rusty-adb/                 # Main application crate
│   ├── src/
│   │   ├── main.rs            # App struct, Message enum, Iced entry point
│   │   ├── lib.rs             # Library target for integration tests
│   │   ├── config.rs          # AppConfig / LogConfig (YAML via serde)
│   │   ├── theme.rs           # ThemeColors + style factory methods
│   │   ├── adb/               # ADB client + domain types (base layer)
│   │   │   ├── mod.rs         #   AdbClient, AdbStatus, AdbDevice, DeviceState
│   │   │   ├── parser.rs      #   ls -la parser (pure functions, unit tested)
│   │   │   └── transfer.rs    #   TransferJob, TransferEvent, TransferStatus, run_transfer
│   │   ├── fs/                # Filesystem abstraction layer
│   │   │   ├── mod.rs         #   FileSystem trait, DirEntry, FsError, PaneState, SortField
│   │   │   ├── local.rs       #   LocalFs implementation
│   │   │   └── android.rs     #   AndroidFs + AndroidContext implementation
│   │   ├── file_pane/         # Generic FilePane<FS> widget
│   │   │   ├── mod.rs         #   State management, rename flow, multi-select
│   │   │   ├── list_view.rs   #   List mode + loading spinner
│   │   │   └── shared_views.rs#   Breadcrumb strip + error state
│   │   ├── views/             # All Iced rendering (depends on adb/, fs/)
│   │   │   ├── mod.rs         #   view(), view_toolbar(), view_panes()
│   │   │   ├── status_bar.rs  #   StatusBar widget
│   │   │   ├── modals.rs      #   Preview, log viewer, about, settings
│   │   │   ├── banners.rs     #   Error, toast, hero banners
│   │   │   ├── pane_controls.rs # Views/Show dropdowns, pane title bars
│   │   │   ├── rendering.rs   #   Grid, icon, columns (Details) layouts
│   │   │   └── setup.rs       #   ADB not-found install guide
│   │   └── update/            # State mutation handlers
│   │       ├── mod.rs         #   update() dispatch + device detection
│   │       ├── pane.rs        #   Navigation, sorting, selection
│   │       ├── file_ops.rs    #   Rename, delete, preview
│   │       ├── transfer.rs    #   Transfer queue, progress, cancel
│   │       ├── install.rs     #   ADB install flow + daemon restart
│   │       └── ui.rs          #   View modes, settings, log viewer, banners
│   └── tests/
│       ├── adb_integration.rs # Integration tests (no real device needed)
│       └── fixtures/mock-adb  # Bash script that fakes the adb binary
├── rusty-logging/             # Shared logging crate (sibling workspace member)
├── scripts/                   # Developer helper shell scripts
├── docs/                      # Architecture docs and screenshots
│   ├── ARCHITECTURE.md        # Detailed module map + data flow
│   └── screenshot.png
├── .beads/                    # Beads issue tracking (do not edit manually)
├── .claude/                   # Agent rules and project context (this file lives here)
├── Cargo.toml                 # Workspace manifest
├── README.md                  # Project overview
├── CONTRIBUTING.md            # Dev setup, testing strategy, conventions
└── AGENTS.md                  # Agent-specific workflow rules
```

---

## Dependency Rules (CRITICAL)

Dependencies must flow strictly downward. Violating this creates circular imports and breaks the architecture:

```
main.rs
  ├── update/      ← mutates App state
  │     └── adb/  ← calls AdbClient methods
  ├── views/       ← renders state into widgets
  │     ├── adb/  ← reads AdbStatus, TransferStatus
  │     └── file_pane/
  ├── file_pane/   ← generic pane widget
  │     └── fs/   ← FileSystem trait, DirEntry
  ├── adb/         ← NO imports from views/, update/, file_pane/
  └── fs/          ← NO imports from views/, update/, file_pane/
```

**Key invariant:** Domain types (`AdbStatus`, `TransferStatus`, `DirEntry`) live at the base of the dependency graph. If a `views/` or `update/` module needs a type, that type belongs in `adb/` or `fs/` — not in the widget module.

---

## Where to Put New Code

| What you're adding | Where it goes |
| --- | --- |
| New ADB command | `adb/mod.rs` (add method to `AdbClient`) |
| New ADB parsing logic | `adb/parser.rs` |
| New domain state type | `adb/mod.rs` (for ADB state) or `fs/mod.rs` (for fs state) |
| New filesystem backend | New file in `fs/` + impl `FileSystem` trait |
| New widget / rendering helper | `views/` — pick the most relevant existing file or add a new one |
| New status bar widget | `views/status_bar.rs` |
| New modal dialog | `views/modals.rs` |
| New `Message` handler | Add the `Message` variant in `main.rs`, add the handler method in the most relevant `update/` file |
| New pane behaviour | `file_pane/mod.rs` (state) + `file_pane/list_view.rs` or `file_pane/shared_views.rs` (view) |
| New config field | `config.rs` |

---

## Logging

rusty-adb uses `tracing` macros throughout. All calls flow through the `rusty-logging` crate, which is initialised once in `main()`:

```rust
rusty_logging::LoggingConfig::builder()
    .level(&config.log.level)
    .console(config.log.console_enabled)
    .file(config.log.file_enabled)
    .build()?;
```

**Use structured fields, not string interpolation:**

```rust
// Good
tracing::warn!(path = %path.display(), error = %e, "delete failed");

// Bad
tracing::warn!("delete failed: {} — {}", path.display(), e);
```

**Level guidance:**

- `error!` — unrecoverable failure or data loss risk
- `warn!` — recoverable error, something the user should know about, silently swallowed OS errors
- `info!` — significant lifecycle event (adb found, device connected, transfer started, settings saved)
- `debug!` — low-level detail useful when diagnosing a specific problem
- `trace!` — hot-path detail (progress ticks, per-byte events) — almost never appropriate

**Never silently swallow errors.** If you use `let _ = some_result`, add a `tracing::warn!` before it. The `if let Err(e) = ...` pattern is preferred:

```rust
if let Err(e) = std::process::Command::new("open").arg(&url).spawn() {
    tracing::warn!(error = %e, url = %url, "failed to open URL");
}
```

---

## Testing

### Running tests

```bash
cargo test --all              # all unit + integration tests
cargo clippy --all-targets -- -D warnings   # must be clean
cargo fmt --all --check       # must have no diffs
```

Or use the one-shot quality gate:

```bash
./scripts/test.sh
```

### Integration tests

Integration tests in `tests/adb_integration.rs` use `tests/fixtures/mock-adb` — a bash script that fakes the `adb` binary. No real Android device is needed. Import paths:

```rust
use rusty_adb::adb::{run_transfer, TransferDirection, TransferEvent, TransferJob};
```

### Unit test locations

Pure functions get `#[cfg(test)]` blocks in their own file. The most important ones:

- `adb/parser.rs` — `parse_ls_output` and `DeviceState::from_str`
- `adb/transfer.rs` — `parse_progress_line`, `parse_speed`, failure path
- `adb/mod.rs` — `AdbStatus::text()` for all variants

---

## Documentation Files

| File | Purpose | When to update |
| --- | --- | --- |
| `README.md` | User-facing overview, install, build, project layout | When adding features or changing the module structure |
| `CONTRIBUTING.md` | Dev setup, testing strategy, code style, PR checklist | When adding new testing patterns or changing conventions |
| `docs/ARCHITECTURE.md` | Deep module map, dependency graph, data flow diagrams | When adding or reorganising modules |
| `AGENTS.md` | Agent-specific beads workflow and session rules | When workflow process changes |
| `.claude/repository-organization.md` | This file — agent-visible repo rules | When reorganising or adding modules |

**Always update docs in the same PR as structural changes.** Stale architecture docs mislead future contributors and agents alike.

---

## Anti-Patterns to Avoid

1. **Domain types in widget modules** — `AdbStatus`, `TransferStatus`, `DirEntry` belong in `adb/` or `fs/`, not in `views/`
2. **Silent error swallowing** — `let _ = spawn()` must be accompanied by a `warn!` call
3. **`tokio::spawn` from `update()`** — use `Task::perform(future, mapper)` instead; keeps update() testable
4. **Monolithic files** — if a file grows past ~300 lines with distinct concerns, split it into module directory form (`foo.rs` → `foo/mod.rs` + `foo/concern.rs`)
5. **Import loops** — `adb/` importing from `views/` or vice versa through some indirect chain

**Last Updated:** 2026-03-13
