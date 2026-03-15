# rusty-adb

```text
╔══════════════════════════════════════════════╗
║  ██████╗ ██╗   ██╗███████╗████████╗██╗   ██╗║
║  ██╔══██╗██║   ██║██╔════╝╚══██╔══╝╚██╗ ██╔╝║
║  ██████╔╝██║   ██║███████╗   ██║    ╚████╔╝ ║
║  ██╔══██╗██║   ██║╚════██║   ██║     ╚██╔╝  ║
║  ██║  ██║╚██████╔╝███████║   ██║      ██║   ║
║  ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝      ╚═╝   ║
║              ─── ADB ───                     ║
║   Android file manager · Rust · Iced         ║
╚══════════════════════════════════════════════╝
```

A native Android file manager for the desktop, built with Rust and [Iced](https://iced.rs).

Browse your Android device's file system, transfer files in both directions, preview images and text, and manage files — all without leaving your terminal workflow.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust Edition 2021](https://img.shields.io/badge/rust-2021_edition-orange.svg)

---

## Screenshot

![screenshot](docs/screenshot.png)

> *Screenshot placeholder — image will appear here once the app is running.*

---

## Features

| Feature | Description |
|---|---|
| **Two-pane browser** | Local filesystem on the left, Android device on the right |
| **Four view modes** | List, Details (columns), Grid, and Icon — switchable per pane |
| **Show dropdown** | Toggle individual columns (Type, Size, Modified) and hidden files per pane |
| **Device auto-detection** | Polls `adb devices` every 2 s; connects automatically when a device is authorised |
| **File transfers** | Copy files to/from Android with a live progress bar and transfer speed display |
| **Copy queue** | Background transfer queue with pause/resume, persistent across restarts, progress in the status bar |
| **Drag-and-drop** | Drop files from Finder/Explorer onto the app to push them to the current Android directory |
| **Multi-file selection** | Select multiple files and transfer them all in one queued batch |
| **APK install** | Select an `.apk` on the local side and install it directly to the connected device |
| **App management** | View all installed apps, uninstall with confirmation — accessible from the pane toolbar |
| **Device details** | View device model, Android version, serial, storage, and battery from the status bar |
| **File preview** | Preview images (PNG, JPG, GIF, WebP, BMP) and text files directly in the app |
| **Rename & delete** | Rename or delete files on the Android device with confirmation prompts |
| **In-app log viewer** | Browse rotating log files with per-level filtering without leaving the app |
| **Settings** | Configure log level, console logging, and file logging via a persistent `config.yml` |
| **About dialog** | App version, GitHub link, and license info |
| **Coloured status bar** | Connection state, live transfer progress, queue activity, and Logs shortcut |

---

## Prerequisites

### Rust

Install via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Minimum supported Rust version: **stable** (1.75+).

### ADB (Android Debug Bridge)

**macOS — Homebrew (recommended):**
```bash
brew install --cask android-platform-tools
```

**macOS — Android Studio:**
The `adb` binary is installed with Android Studio at:
```
~/Library/Android/sdk/platform-tools/adb
```
rusty-adb checks this path automatically if `adb` is not on your `$PATH`.

**Linux:**
```bash
# Debian/Ubuntu
sudo apt install android-tools-adb

# Arch
sudo pacman -S android-tools
```

**Windows:** Download [Platform Tools](https://developer.android.com/tools/releases/platform-tools) and add the folder to your `PATH`.

### Android Device Setup

1. On your Android phone, go to **Settings → About phone** and tap **Build number** 7 times to enable Developer Options.
2. Go to **Settings → Developer options** and enable **USB Debugging**.
3. Connect the phone with a USB cable.
4. When prompted on the phone, tap **Allow** to authorise your computer.

---

## Build & Run

### Quick start

```bash
git clone https://github.com/aaroncroberts/rusty-adb.git
cd rusty-adb

# Run in development mode
./scripts/dev.sh

# Or build a release binary
./scripts/build.sh
./target/release/rusty-adb
```

### Manual cargo commands

```bash
# Development build + run
cargo run --package rusty-adb

# Release build
cargo build --release
./target/release/rusty-adb

# Run all tests
cargo test --all

# Lint
cargo clippy --all-targets -- -D warnings
```

### Build a release binary and install

```bash
# Build the release binary
./scripts/build.sh
# The binary is at: ./target/release/rusty-adb

# Install to ~/bin (default) or a custom prefix
./scripts/install.sh
./scripts/install.sh --prefix ~/.local/bin

# Or install with cargo directly
cargo install --path rusty-adb
```

---

## Developer Scripts

The `scripts/` directory provides richly-formatted helper scripts for common tasks:

| Script | Purpose |
|---|---|
| `scripts/build.sh` | Release build (`--debug` for a debug binary) |
| `scripts/test.sh` | Full quality gate: tests → clippy → fmt |
| `scripts/dev.sh` | Run with auto-reload via `cargo-watch` (falls back to `cargo run`) |
| `scripts/install.sh` | Build and install the release binary |
| `scripts/check-deps.sh` | Prerequisite checker (sourced by all other scripts) |

All scripts accept `--help`.

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `F5` | Refresh both panes |
| `F2` | Rename selected Android file |
| `Backspace` | Navigate up in the local pane |
| `S` | Open Settings |
| `Escape` | Close modal (log viewer → settings → about → preview → rename cancel → clear error) |

---

## Using the Interface

### View Modes

Each pane has its own **Views** dropdown in the toolbar. Options:

| Mode | Description |
|---|---|
| **List** | Single-column filename rows — highest density |
| **Details** | Tabular rows with Name, Size, Modified, Type columns |
| **Grid** | Compact multi-column tiles (4 per row) |
| **Icon** | Large tiles (2–3 per row) with big type icon |

### Show Dropdown

The **Show** dropdown next to the Views picker lets you toggle what each pane displays:

- **Type** — file type column (Details mode)
- **Size** — file size column (Details mode)
- **Modified** — last-modified date column (Details mode)
- **Hidden files** — toggle display of dotfiles (`.filename`)

Items with a `✓` prefix are currently enabled; clicking them again disables the setting.

---

## Configuration

rusty-adb reads `~/.rusty-adb/config.yml` on startup. All fields are optional and fall back to sensible defaults.

```yaml
log:
  level: info              # trace | debug | info | warn | error
  console_enabled: true    # write log output to stderr
  file_enabled: true       # write rotating log files to ~/.rusty-adb/
```

Log files are written to `~/.rusty-adb/` and rotate automatically.

---

## Project Layout

```
rusty-adb/
├── rusty-adb/             # Main application crate
│   ├── src/
│   │   ├── main.rs        # Iced Application, App struct, Message enum
│   │   ├── lib.rs         # Library target (re-exports for integration tests)
│   │   ├── config.rs      # AppConfig loaded from config.yml
│   │   ├── theme.rs       # ThemeColors palette + style factory methods
│   │   ├── icons.rs       # Nerd Font icon constants + font helpers
│   │   ├── adb/           # ADB client, domain types, transfer engine
│   │   │   ├── mod.rs     #   AdbClient, AdbStatus, AdbDevice, InstalledApp
│   │   │   ├── parser.rs  #   ls -la output parser (pure, fully tested)
│   │   │   └── transfer.rs#   TransferJob, TransferEvent, TransferStatus, run_transfer
│   │   ├── fs/            # Filesystem abstraction
│   │   │   ├── mod.rs     #   FileSystem trait, DirEntry, FsError, PaneState, SortField
│   │   │   ├── local.rs   #   LocalFs (host filesystem backend)
│   │   │   └── android.rs #   AndroidFs, AndroidContext (ADB backend)
│   │   ├── queue/         # Copy queue: domain model, persistence, manager
│   │   │   └── mod.rs     #   QueueItem, QueueStatus, QueueManager, QueueSummary
│   │   ├── file_pane/     # Generic two-pane widget
│   │   │   ├── mod.rs     #   FilePane<FS> struct, state management, rename/select
│   │   │   ├── list_view.rs      # List-mode and loading-spinner views
│   │   │   └── shared_views.rs   # Breadcrumb strip, error state widget
│   │   ├── views/         # All Iced widget rendering
│   │   │   ├── mod.rs     #   view(), delayed_tip(), modal_backdrop(), view_panes()
│   │   │   ├── status_bar.rs     # StatusBar widget (connection, transfer, queue)
│   │   │   ├── modals.rs         # Preview, queue dialog, device details, log viewer, settings
│   │   │   ├── banners.rs        # Error, toast banners; view_header() toolbar
│   │   │   ├── pane_controls.rs  # Views/Show dropdowns, pane title bars
│   │   │   ├── rendering.rs      # Grid, icon, columns (Details) layouts
│   │   │   └── setup.rs          # ADB not-found install guide
│   │   └── update/        # State mutation handlers (one impl App block each)
│   │       ├── mod.rs     #   update() dispatch, device detection, derive_adb_status
│   │       ├── pane.rs    #   Navigation, sorting, selection for both panes
│   │       ├── file_ops.rs#   Rename, delete, preview
│   │       ├── transfer.rs#   Transfer queue, progress, complete, cancel
│   │       ├── apps.rs    #   APK install, package list, uninstall
│   │       ├── queue.rs   #   Copy queue pause/resume/clear handlers
│   │       ├── install.rs #   ADB install flow, daemon restart
│   │       └── ui.rs      #   View modes, settings, log viewer, banners, escape routing
│   └── tests/
│       ├── adb_integration.rs    # Integration tests via mock-adb (no real device needed)
│       └── fixtures/mock-adb     # Bash script that fakes the adb binary
├── rusty-logging/         # Shared logging crate (rotating file + console via tracing)
├── scripts/               # Developer helper scripts
└── docs/                  # Architecture docs and screenshots
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup, coding conventions, the test strategy (unit tests + mock-adb integration tests), and the PR checklist.

For a deeper look at the architecture, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## License

MIT — see [LICENSE](LICENSE).
