# rusty-adb Documentation

```text
◄◄ RUSTY-ADB ►► — Android file manager · Rust + Iced
```

Welcome to the rusty-adb user documentation. Whether you're setting up for the first time or looking for details on a specific feature, this is the right place to start.

---

## Table of Contents

| | Guide | What's inside |
|---|---|---|
| 🚀 | [Getting Started](getting-started.md) | Install, ADB setup, connect a device, first file transfer |
| 📂 | [File Browser](file-browser.md) | Two-pane layout, view modes, sorting, multi-select, navigation |
| 📤 | [File Transfers](file-transfers.md) | Push, pull, copy queue, drag-and-drop, cancel |
| 📦 | [APK & App Management](apk-management.md) | Install APKs, list installed apps, uninstall |
| 📱 | [Device Details](device-details.md) | Device info, storage, network, battery |
| ⚙️ | [Settings & Logs](settings.md) | Configuration, log levels, in-app log viewer |

---

## Quick Overview

rusty-adb is a native desktop Android file manager built with Rust and [Iced](https://iced.rs). It connects to any Android device over USB (or TCP/IP) via ADB and gives you a fast, keyboard-friendly two-pane interface for browsing, transferring, and managing files.

**Key capabilities at a glance:**

- **Two-pane browser** — local filesystem on the left, Android device on the right
- **Four view modes** — List, Details, Grid, Icon — switchable per pane independently
- **File transfers** — push files to Android, pull files to your computer, with live progress and speed
- **Copy queue** — queue multiple transfers, pause/resume, survives app restarts
- **Drag-and-drop** — drag files from Finder or Explorer onto the app to push them instantly
- **APK install** — select any `.apk` on your local side and install it to the connected device
- **App management** — browse installed apps and uninstall with a confirmation step
- **Device details** — model, Android version, storage breakdown, battery, IP address
- **File preview** — images (PNG, JPG, GIF, WebP, BMP) and text files without leaving the app
- **File operations** — rename and delete Android files with confirmation prompts
- **In-app log viewer** — browse rotating log files with per-level filtering
- **Settings** — configure logging via `~/.rusty-adb/config.yml`

---

## Where to Go Next

**New to rusty-adb?** Start with [Getting Started](getting-started.md) — it walks you through everything from install to your first file transfer.

**Looking for a specific feature?** Jump directly to the relevant guide in the table above.

**Developer or contributor?** See the resources below.

---

## Developer Resources

| Resource | Purpose |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Module map, dependency graph, Elm/Iced pattern, data flow |
| [../CONTRIBUTING.md](../CONTRIBUTING.md) | Dev environment setup, testing strategy, PR checklist |
| [../README.md](../README.md) | Project overview, build commands, project layout |

---

## Getting Help

If something isn't covered here or you've found a bug, please [open an issue](https://github.com/aaroncroberts/rusty-adb/issues) on GitHub.
