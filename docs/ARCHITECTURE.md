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
- `modal_state` — which overlay is showing (preview / about / settings / none)
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

### update() (`main.rs`)

`App::update()` is the single place state transitions happen. It:

1. Matches the incoming `Message`
2. Mutates `self` (state)
3. Returns a `Task<Message>` for any async side effect (or `Task::none()`)

Side effects are always expressed as `Task::perform(future, mapper)` — never spawned directly. This keeps `update()` testable.

### view() (`main.rs`)

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
        status_bar.view(…),
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

### `main.rs` — Application shell

- Defines `App`, `Message`, and all `update()` / `view()` logic
- Entry point: `iced::application(…).run_with(…)`
- Owns the `TransferJob` queue and cancel flag
- All modal view helpers (`view_preview_modal`, `view_about_modal`, `view_settings_modal`)
- Keyboard handler (`handle_key_press`)

### `adb.rs` — ADB client

- `AdbClient` — wraps the path to the `adb` binary; all methods are `async`
- `find()` — locates `adb` in `$PATH` or `~/Library/Android/sdk/platform-tools/adb`
- `list_devices()` — runs `adb devices -l`, parses `AdbDevice` list
- `list_dir()` / `list_dir_with_roots()` — runs `adb shell ls -la`, returns `Vec<AndroidEntry>`
- `rename()` / `delete()` — fire-and-forget shell commands via `check_adb_output()` helper
- `pull_to_temp()` — copies a remote file to a temp path for preview
- `start_server()` / `disconnect()` — daemon lifecycle
- `parse_ls_output()` — pure function, parses `ls -la` text into `AndroidEntry` structs

### `transfer.rs` — File transfer engine

- `TransferJob` — describes a single push/pull: source, destination, direction, serial
- `TransferEvent` — events emitted during a transfer: `Progress { percent, speed }`, `Complete`, `Failed`, `Cancelled`
- `run_transfer()` — async function that spawns `adb push/pull --progress`, streams stderr line-by-line through a callback, and honours the cancel flag
- `parse_progress_line()` / `parse_speed()` — pure parsers for adb progress output

### `config.rs` — Application configuration

- `AppConfig` — top-level config struct, loaded from `~/.rusty-adb/config.yml`
- `LogConfig` — `level`, `console_enabled`, `file_enabled`
- `AppConfig::load()` — reads YAML, falls back to defaults on any error

### `local_pane.rs` — Local filesystem pane

- `LocalPane` — holds current directory, `Vec<FileEntry>`, sort state, hidden file toggle
- `view()` — renders the left pane widget tree
- `SortField` — `Name | Size | Modified`

### `android_pane.rs` — Android device pane

- `AndroidPane` — state machine: `NoDevice | Unauthorized | Loading | Browsing | Error`
- `view()` — renders the right pane, including inline rename text input
- Entry selection and multi-select tracking

### `status_bar.rs` — Status bar widget

- `AdbStatus` — `NotFound | Disconnected | Unauthorized | Connecting | Connected | Error`
- `TransferStatus` — live transfer state: filename, percent, speed, queue position
- `StatusBar::view()` — renders either connection text or progress bar + cancel button

### `theme.rs` — Colour palette

- `ThemeColors` — a flat struct of `iced::Color` values used by all widgets
- Derived from Iced's `Theme::TokyoNightStorm`

### `lib.rs` — Library target

- Exposes `pub mod adb` and `pub mod transfer` so integration tests in `tests/` can import them without going through the binary target

---

## Data Flow

### Device detection

```
Subscription (every 2 s)
  → Message::PollDevices
  → Task::perform(adb_client.list_devices())
  → Message::DevicesLoaded(devices)
  → App::update() — picks first authorised device
  → Message::AndroidNavigateTo(PathBuf::from("/sdcard"))
  → Task::perform(adb_client.list_dir_with_roots())
  → Message::AndroidEntriesLoaded { path, entries, roots }
  → AndroidPane transitions to Browsing
```

### File transfer

```
User clicks "→ Android" or drops files
  → Message::CopyToAndroid (or FileDropped)
  → App::update() — builds Vec<TransferJob>, pushes to transfer_queue
  → pops first job, spawns transfer Subscription
  → Subscription::run_with_id(run_transfer, cancel_flag, |ev| …)
  → TransferEvent::Progress → Message::TransferProgress { percent }
     → updates TransferStatus in StatusBar
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
  → Task::perform(adb_client.pull_to_temp(serial, remote_path))
  → Message::PreviewReady(local_path)
     → if image extension: ModalState::ImagePreview(local_path)
     → else: read file, ModalState::TextPreview(content)
  → App::view() renders view_preview_modal() on top via stack!
```

---

## Async Strategy

- All I/O goes through `Task::perform(future, msg_mapper)` — never `tokio::spawn` from `update()`.
- File transfers use `iced::stream::channel` to produce a `Subscription` that streams `TransferEvent`s as the adb process runs.
- Directory listings and ADB device polls use `Task::perform` with `spawn_blocking` wrappers inside `adb.rs` (the `adb_client` crate's sync API is wrapped in `tokio::task::spawn_blocking`).
- The cancel flag (`Arc<AtomicBool>`) is checked between each read in `run_transfer` — this gives cooperative cancellation without killing the process forcefully.

---

## Testing Architecture

| Layer | Location | What it tests | Device needed? |
|---|---|---|---|
| Unit | `src/*.rs` (in `#[cfg(test)]`) | Pure functions, state transitions | No |
| Integration | `tests/adb_integration.rs` | `AdbClient` + `run_transfer` end-to-end | No (uses mock-adb) |
| Manual | — | UI rendering, real device transfers | Yes |

### mock-adb (`tests/fixtures/mock-adb`)

A bash script that implements the minimum `adb` interface required by `AdbClient`:

- Returns synthetic `devices -l` output
- Simulates `ls -la` output for `/sdcard` and subdirectories
- Emits fake `[ XX%]` progress lines on stderr for push/pull
- Handles `start-server`, `disconnect`, `shell mv`, `shell rm`

Integration tests construct `AdbClient` with `adb_path = mock_adb_path()` (resolved from `CARGO_MANIFEST_DIR`) and run fully offline.
