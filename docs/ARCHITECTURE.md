# Architecture — rusty-adb

This document describes the high-level architecture of rusty-adb, the Iced Elm-like pattern it follows, and the responsibilities of each module.

---

## Table of Contents

- [UI Framework — Iced 0.13](#ui-framework--iced-013)
- [The Elm Pattern in rusty-adb](#the-elm-pattern-in-rusty-adb)
- [Module Map](#module-map)
- [Data Flow](#data-flow)
- [Async Strategy](#async-strategy)
- [Testing Architecture](#testing-architecture)

---

## UI Framework — Iced 0.13

rusty-adb is built on [Iced](https://iced.rs), a cross-platform GUI framework for Rust inspired by the Elm architecture. Iced 0.13 uses a `Task`-based effect system and `Subscription` for ongoing event streams.

Key Iced concepts used in this codebase:

| Concept | Iced type | Role in rusty-adb |
|---|---|---|
| State | `App` struct | All mutable application state |
| Events | `Message` enum | Every user action and async result |
| State transitions | `App::update()` | Pure-ish function: `(State, Message) → (State, Task<Message>)` |
| UI construction | `App::view()` | Pure function: `&State → Element<Message>` |
| Background work | `Task<Message>` | Async adb calls, file transfers |
| Ongoing streams | `Subscription<Message>` | Device polling, spinner tick, transfer progress |

---

## The Elm Pattern in rusty-adb

```
                    ┌──────────────────────────────┐
                    │           App::view()         │
                    │  &State → Element<Message>    │
                    └───────────────┬──────────────┘
                                    │ user action / timer
                                    ▼
                    ┌──────────────────────────────┐
                    │         Message enum          │
                    │  (e.g. CopyToAndroid,         │
                    │   TransferProgress { 42% },   │
                    │   DevicesLoaded([…]))         │
                    └───────────────┬──────────────┘
                                    │
                                    ▼
                    ┌──────────────────────────────┐
                    │        App::update()          │
                    │  mutates State, returns       │
                    │  Task<Message> for side       │
                    │  effects                      │
                    └───────────────┬──────────────┘
                                    │ Task completes → new Message
                                    ▼
                              (loop back to view)
```

### State (`App` struct, `main.rs`)

All application state is owned by the `App` struct:

- `adb_client` — optional reference-counted `AdbClient`
- `android_pane` — pane state machine (NoDevice → Loading → Browsing / Error)
- `local_pane` — local filesystem state
- `transfer_queue` — `VecDeque<TransferJob>` for batched transfers
- `cancel_flag` — shared `Arc<AtomicBool>` for cooperative cancellation
- `preview_modal` / `about_open` / `settings_open` — overlay state
- `adb_status` — connection state shown in the status bar
- `config` — loaded from `~/.rusty-adb/config.yml`

### Messages (`Message` enum, `main.rs`)

Every event in the system — user clicks, timer ticks, async results — is a `Message`. Groups:

- **ADB lifecycle**: `AdbReady`, `PollDevices`, `DevicesLoaded`, `AdbError`, `AdbNotFound`
- **Local pane**: `LocalNavigateTo`, `LocalSelectEntry`, `LocalToggleHidden`, `LocalSortBy`
- **Android pane**: `AndroidNavigateTo`, `AndroidSelectEntry`, `AndroidEntriesLoaded`, `AndroidBeginRename`, `AndroidRenameCommit`, `AndroidBeginDelete`, `AndroidDeleteConfirm`
- **Transfers**: `CopyToAndroid`, `CopyToLocal`, `TransferProgress`, `TransferComplete`, `TransferFailed`, `TransferCancelled`, `CancelTransfer`
- **Modals**: `PreviewFile`, `PreviewReady`, `ClosePreview`, `OpenAbout`, `CloseAbout`, `OpenSettings`, `CloseSettings`, `SaveSettings`
- **System**: `FileDropped`, `FileHovered`, `FilesHoveredLeft`, `EscapePressed`, `RefreshPanes`

### update() (`update/mod.rs`)

`App::update()` is the single place state transitions happen. It:

1. Matches the incoming `Message`
2. Delegates to a focused handler in one of the `update/` sub-modules
3. Returns a `Task<Message>` for any async side effect (or `Task::none()`)

Side effects are always expressed as `Task::perform(future, mapper)` — never spawned directly. This keeps `update()` testable.

### view() (`views/mod.rs`)

`App::view()` constructs the widget tree from the current state. It never mutates state. The layout is:

```
stack![
    column![
        view_toolbar(),
        row![
            local_pane.view(…),
            vertical_rule(1),
            android_pane.view(…),
        ],
        StatusBar::view(…),
    ],
    // modals rendered on top via stack (only when active):
    view_preview_modal(…),   // or
    view_about_modal(),      // or
    view_settings_modal(),
]
```

Modals are rendered as full-window overlays using `modal_backdrop()` — a private helper that wraps any card widget in a semi-transparent `container`.

---

## Module Map

The source tree is organised into focused module directories. Each directory has a `mod.rs` that owns the public API and may delegate to sibling files for large or distinct concerns.

```
rusty-adb/
├── rusty-adb/            # Main application crate
│   └── src/
│       ├── main.rs       # Iced entry point, App struct, Message enum
│       ├── lib.rs        # Library target (re-exports for integration tests)
│       ├── config.rs     # AppConfig loaded from config.yml
│       ├── theme.rs      # ThemeColors palette
│       ├── adb/          # ADB client and domain types
│       ├── fs/           # Filesystem abstraction layer
│       ├── file_pane/    # Generic pane widget
│       ├── views/        # All rendering / widget code
│       └── update/       # All state-mutation handlers
└── rusty-logging/        # Shared logging crate (sibling workspace member)
```

---

### `main.rs` — Application shell

- Defines `App` and `Message`
- Entry point: `iced::application(…).run_with(…)`
- Owns the `TransferJob` queue and cancel flag
- Wires subscriptions: device poll, spinner tick, transfer progress channel
- Delegates all business logic to `update/` handlers and all rendering to `views/`

---

### `adb/` — ADB client and domain types

The ADB module is the lowest-level layer. Nothing in `adb/` imports from `views/`, `update/`, or `file_pane/`.

#### `adb/mod.rs`

- `AdbClient` — wraps the path to the `adb` binary; all methods are `async`
- `AdbDevice` / `DeviceState` — parsed device list entries
- `AdbStatus` — domain enum: `NotFound | Disconnected | Unauthorized | Connecting | Connected | Error`
  - Lives here (not in the status bar widget) so `update/` modules can set it without importing from `views/`
- `find()` — locates `adb` in `$PATH` or `~/Library/Android/sdk/platform-tools/adb`
- `list_devices()` — runs `adb devices -l`, parses `AdbDevice` list
- `list_dir()` — runs `adb shell ls -la`, returns `Vec<AndroidEntry>`
- `rename()` / `delete()` — shell commands via `check_adb_output()` helper
- `pull_to_temp()` — copies a remote file to a temp path for preview
- `start_server()` / `disconnect()` — daemon lifecycle

#### `adb/parser.rs`

- `parse_ls_output()` — pure function, parses `ls -la` text into `AndroidEntry` structs
- All parsing logic is isolated here for testability (no I/O, no side effects)

#### `adb/transfer.rs`

- `TransferJob` — describes a single push/pull: source, destination, direction, serial
- `TransferEvent` — events emitted during a transfer: `Progress { percent, speed }`, `Complete`, `Failed`, `Cancelled`
- `TransferStatus` — live snapshot of the running transfer (filename, percent, speed display, queue position)
  - Lives here (not in the status bar widget) so `update/` can build it without importing from `views/`
- `TransferDirection` — `Push | Pull`
- `run_transfer()` — async function that spawns `adb push/pull --progress`, streams stderr line-by-line, and honours the cancel flag
- `parse_progress_line()` / `parse_speed()` — pure parsers for adb progress output

All public types are re-exported from `adb/mod.rs`:

```rust
pub use transfer::{run_transfer, TransferDirection, TransferEvent, TransferJob, TransferStatus};
```

---

### `fs/` — Filesystem abstraction

The `fs/` module defines the shared trait and types that both the local and Android panes build on. Callers depend on the trait, not on either concrete implementation.

#### `fs/mod.rs`

- `FileSystem` trait — `async fn list_dir(&self, ctx: &Self::Context, path: &Path) → Result<Vec<DirEntry>, FsError>`
- `DirEntry` — unified file/directory representation (name, path, size, modified, is_dir)
- `FsError` — wrapper around I/O and ADB errors
- `PaneState<FS>` — generic pane state machine (Idle → Loading → Browsing / Error)
- `SortField` — `Name | Size | Modified | Type`
- `android_entry_to_dir_entry()` — converts `AndroidEntry` (ADB) to the shared `DirEntry`

#### `fs/local.rs`

- `LocalFs` — `FileSystem` impl for the host filesystem
- Uses `std::fs::read_dir` under `tokio::task::spawn_blocking`

#### `fs/android.rs`

- `AndroidFs` — `FileSystem` impl that delegates to `AdbClient`
- `AndroidContext` — holds the `AdbClient` and device serial needed per call

---

### `file_pane/` — Generic pane widget

`FilePane<FS>` is a generic, reusable pane that works for both the local and Android sides. Rendering is separate from state management.

#### `file_pane/mod.rs`

- `FilePane<FS>` struct — holds current path, entries, sort state, selection set, rename pending state
- `begin_navigate()` / `on_entries_loaded()` / `on_error()` — state transitions
- `begin_rename()` / `update_rename_input()` / `cancel_rename()` — inline rename flow
- `select()` / `toggle_select()` — multi-select logic
- `set_sort()` / `sorted_entries()` — sort management
- `RENAME_INPUT_ID` — `text_input::Id` used by the rename field

#### `file_pane/list_view.rs`

- `view_list()` — renders the entry list (List mode)
- `view_loading()` — spinner placeholder shown during directory loads

#### `file_pane/shared_views.rs`

- `view_breadcrumb()` — clickable path breadcrumb strip
- `view_error()` — error state widget

---

### `views/` — Widget and rendering layer

All Iced widget construction lives here. The `views/` module imports from `adb/` and `fs/` but nothing in `adb/` or `fs/` imports from `views/`. Dependency arrows go one way only.

#### `views/mod.rs`

- `view()` — top-level Iced view function; builds the full widget tree
- `view_toolbar()` — top toolbar with device picker, transfer controls, and menus
- `view_panes()` — the two-pane split layout

#### `views/status_bar.rs`

- `StatusBar` — stateless widget struct
- `StatusBar::view()` — renders either connection text or transfer progress bar + cancel button
- Imports `AdbStatus` and `TransferStatus` from `crate::adb` — never defines them
- `STATUS_BAR_HEIGHT: f32 = 30.0`

#### `views/modals.rs`

- `view_preview_modal()` — image or text file preview overlay
- `view_about_modal()` — app info, version, GitHub link
- `view_settings_modal()` — log level and file/console logging toggles
- `view_log_viewer_modal()` — in-app log viewer with file selector and level filter

#### `views/banners.rs`

- `view_error_banner()` — red error strip (auto-dismisses after 3 s)
- `view_toast_banner()` — info toast strip (auto-dismisses after 3 s)
- `view_hero_banner()` — large placeholder shown when no device is connected

#### `views/pane_controls.rs`

- `view_menu_bar()` — Views / Show dropdowns per pane
- `view_pane_title_bar()` — pane header with current path and navigation controls

#### `views/rendering.rs`

- `view_grid()` — compact multi-column tile layout
- `view_icon()` — large icon tile layout
- `view_columns()` — Details mode (tabular with Name, Size, Modified, Type)
- Icon helpers and column formatting utilities

#### `views/setup.rs`

- `view_adb_not_found()` — full-screen ADB install guide
- Platform-specific install instructions (Homebrew, winget, manual)

---

### `update/` — State mutation handlers

`App::update()` in `main.rs` dispatches every `Message` to one of these modules. Each module is an `impl App` block — no structs, just focused methods. Tests can call these methods directly without Iced plumbing.

#### `update/mod.rs`

- `App::update()` — the full `Message` dispatch table
- `derive_adb_status()` — converts device list into `AdbStatus`
- Device detection: `poll_devices()`, `devices_loaded()`, `adb_ready()`, `adb_error()`

#### `update/pane.rs`

- Local pane: `local_navigate_to()`, `local_entries_loaded()`, `local_load_error()`, `local_select_entry()`
- Local sorting: `local_sort_by()`, `local_toggle_hidden()`, `local_toggle_type()`, `local_toggle_size()`, `local_toggle_modified()`
- Android pane: `android_navigate_to()`, `android_entries_loaded()`, `android_load_error()`, `android_select_entry()`
- Android sorting: `android_sort_by()`, `android_toggle_*()` variants
- Shared: `refresh_panes()`, `disconnect_device()`, `spinner_tick()`

#### `update/file_ops.rs`

- Rename: `android_begin_rename()`, `android_rename_input()`, `android_rename_cancel()`, `android_rename_commit()`
- Delete: `android_begin_delete()`, `android_delete_cancel()`, `android_delete_confirm()`
- Preview: `preview_file()`, `preview_ready()`, `preview_failed()`, `close_preview()`

#### `update/transfer.rs`

- `copy_to_android()` / `copy_to_local()` — builds `TransferJob` list from selected entries
- `transfer_progress()` — updates `TransferStatus` on each progress tick
- `transfer_complete()` — pops next job from the queue or clears transfer state
- `transfer_failed()` / `transfer_cancelled()` — error and cancel paths
- `cancel_transfer()` — sets the `Arc<AtomicBool>` cancel flag

#### `update/install.rs`

- `adb_not_found()` — sets `AdbStatus::NotFound`
- `retry_adb_find()` — re-runs `AdbClient::find()` after manual install
- `install_adb()` — invokes the platform package manager (Homebrew / winget)
- `install_complete()` / `install_failed()` — install outcome handlers
- `daemon_start_failed()` / `restart_daemon()` — ADB daemon lifecycle
- `open_url()` — opens a URL in the system browser

#### `update/ui.rs`

- View mode: `set_local_view_mode()`, `set_android_view_mode()`, `local_gallery_select()`, `android_gallery_select()`
- About: `open_about()`, `close_about()`
- Settings: `open_settings()`, `close_settings()`, `settings_draft_*()`, `save_settings()`
- Log viewer: `open_log_viewer()`, `close_log_viewer()`, `log_viewer_files_loaded()`, `log_viewer_select_file()`, `log_viewer_file_loaded()`, `log_viewer_set_level()`
- Banners: `show_error()`, `dismiss_error()`, `show_toast()`, `dismiss_toast()`
- Keyboard: `escape_pressed()` — cascades ESC through log viewer → settings → about → preview → rename cancel

---

### `config.rs` — Application configuration

- `AppConfig` — top-level config struct, loaded from `~/.rusty-adb/config.yml`
- `LogConfig` — `level`, `console_enabled`, `file_enabled`
- `AppConfig::load()` — reads YAML, falls back to defaults on any error

---

### `theme.rs` — Colour palette

- `ThemeColors` — a flat struct of `iced::Color` values used across all widget modules
- Derived from Iced's `Theme::TokyoNightStorm`
- Nine `style_*()` factory methods for container, button, text, rule, and scrollable styling

---

### `lib.rs` — Library target

- Re-exports `pub mod adb` and `pub mod fs` so integration tests in `tests/` can import them without going through the binary target
- Keeps the test-visible surface minimal

---

### `rusty-logging/` — Shared logging crate

A sibling workspace member (`../rusty-logging`) that provides structured, rotating log file support on top of `tracing`.

- `LoggingConfig::builder()` — fluent builder for log level, console output, and file rotation
- Initialised once in `main()` via `rusty_logging::LoggingConfig::builder().build()`
- All `tracing::info!` / `tracing::warn!` / `tracing::error!` calls in the app automatically flow through it — no per-callsite setup needed

---

## Dependency Graph

Dependencies flow strictly downward. No upward or sideways imports are allowed across these tiers:

```
main.rs
   ├── update/          ← dispatches messages, mutates App
   │      └── adb/      ← calls AdbClient methods
   ├── views/           ← renders state to widgets
   │      └── adb/      ← reads AdbStatus, TransferStatus (domain types)
   │      └── file_pane/ ← embeds pane views
   ├── file_pane/       ← generic pane widget
   │      └── fs/       ← FileSystem trait, DirEntry
   ├── adb/             ← ADB client + domain types (no app imports)
   ├── fs/              ← filesystem abstraction (no app imports)
   └── config.rs / theme.rs / lib.rs
```

Key invariant: `adb/` and `fs/` do not import from `views/`, `update/`, or `file_pane/`. Domain types (`AdbStatus`, `TransferStatus`, `DirEntry`) live at the bottom of the dependency graph so every layer can use them without circular imports.

---

## Data Flow

### Device detection

```
Subscription (every 2 s)
  → Message::PollDevices
  → Task::perform(adb_client.list_devices())
  → Message::DevicesLoaded(devices)
  → update/mod.rs: derive_adb_status(), picks first authorised device
  → Message::AndroidNavigateTo(PathBuf::from("/sdcard"))
  → Task::perform(adb_client.list_dir() + list_storage_roots()) [parallel]
  → Message::AndroidEntriesLoaded { path, entries, roots }
  → android_pane transitions to Browsing
```

### File transfer

```
User clicks "→ Android" or drops files
  → Message::CopyToAndroid (or FileDropped)
  → update/transfer.rs: builds Vec<TransferJob>, pushes to transfer_queue
  → pops first job, starts transfer Subscription
  → iced::stream::channel → adb/transfer.rs: run_transfer()
  → TransferEvent::Progress → Message::TransferProgress { percent }
     → update/transfer.rs: builds TransferStatus, stored in App
     → views/status_bar.rs: renders progress bar from TransferStatus
  → TransferEvent::Complete → Message::TransferComplete { speed_display }
     → pops next job from queue, or clears transfer state
  → TransferEvent::Failed → Message::TransferFailed(msg)
     → shows error toast, aborts queue
  → TransferEvent::Cancelled → Message::TransferCancelled
     → clears queue and transfer state
```

### File preview

```
User double-clicks an Android entry (image or text)
  → Message::PreviewFile(entry)
  → update/file_ops.rs: Task::perform(adb_client.pull_to_temp(serial, remote_path))
  → Message::PreviewReady(local_path)
     → if image extension: preview_modal = Some(PreviewContent::Image(path))
     → else: read file, preview_modal = Some(PreviewContent::Text(content))
  → views/modals.rs: view_preview_modal() renders on top via stack!
```

---

## Async Strategy

- All I/O goes through `Task::perform(future, msg_mapper)` — never `tokio::spawn` from `update()`.
- File transfers use `iced::stream::channel` to produce a `Subscription` that streams `TransferEvent`s as the adb process runs.
- Directory listings and ADB device polls use `Task::perform` with `spawn_blocking` wrappers (the `adb_client` crate's sync API is wrapped in `tokio::task::spawn_blocking`).
- The cancel flag (`Arc<AtomicBool>`) is checked between each read in `run_transfer` — this gives cooperative cancellation without killing the process forcefully.
- Dropped progress ticks (channel full during a fast transfer) are logged at `warn!` level and do not crash the transfer.

---

## Testing Architecture

| Layer | Location | What it tests | Device needed? |
|---|---|---|---|
| Unit | `src/**/*.rs` (in `#[cfg(test)]`) | Pure functions, state transitions | No |
| Integration | `tests/adb_integration.rs` | `AdbClient` + `run_transfer` end-to-end | No (uses mock-adb) |
| Manual | — | UI rendering, real device transfers | Yes |

### Unit test locations

| Module | Tests cover |
|---|---|
| `adb/parser.rs` | `parse_ls_output`, `DeviceState::from_str`, display formatting |
| `adb/transfer.rs` | `parse_progress_line`, `parse_speed`, failure path on bad exit code |
| `adb/mod.rs` | `AdbStatus::text()` output for all variants |
| `config.rs` | YAML parsing, default fallbacks |
| `fs/mod.rs` | `SortField` ordering, `DirEntry` formatting |

### Integration tests (`tests/adb_integration.rs`)

These use a **mock-adb** bash script (`tests/fixtures/mock-adb`) instead of a real `adb` binary. The mock implements the minimum subset of `adb` needed to test `AdbClient`:

- `devices -l` — returns a fake authorised device
- `shell ls -la` — returns synthetic directory listings for `/sdcard` and subdirectories
- `push`/`pull --progress` — emits fake `[ XX%]` progress lines on stderr
- `shell mv`, `shell rm -rf`, `disconnect`, `start-server`

`AdbClient` is constructed with an explicit `adb_path` pointing at the mock, so no real Android device or adb daemon is needed. Tests run fully offline.

**Import path for integration tests:**

```rust
use rusty_adb::adb::{run_transfer, TransferDirection, TransferEvent, TransferJob};
```

**Adding a new integration test:**

1. Add a `#[tokio::test]` function in `tests/adb_integration.rs`.
2. Use `make_client()` (already defined in that file) to get an `AdbClient` backed by `mock-adb`.
3. If the mock needs to handle a new command, add a `case` branch in `tests/fixtures/mock-adb`.
