# Getting Started

> **New to rusty-adb?** This guide walks you through everything — from installing the prerequisites to transferring your first file.

---

## Table of Contents

- [Prerequisites](#prerequisites)
  - [Rust](#1-rust)
  - [ADB (Android Debug Bridge)](#2-adb-android-debug-bridge)
- [Set Up Your Android Device](#set-up-your-android-device)
- [Build and Run](#build-and-run)
- [Install a Release Binary](#install-a-release-binary)
- [Connect Your Device](#connect-your-device)
- [Your First File Transfer](#your-first-file-transfer)
- [Next Steps](#next-steps)

---

## Prerequisites

### 1. Rust

rusty-adb is built with Rust. Install the Rust toolchain via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Minimum supported version: **Rust stable 1.75+**.
After install, open a new terminal and verify:

```bash
rustc --version   # rustc 1.75.0 or higher
```

---

### 2. ADB (Android Debug Bridge)

ADB is the bridge between rusty-adb and your Android device.
rusty-adb will detect `adb` automatically in your `$PATH` or at `~/Library/Android/sdk/platform-tools/adb` (macOS).

#### macOS

**Homebrew (recommended):**
```bash
brew install --cask android-platform-tools
```

**Android Studio:**
ADB ships with Android Studio at:
```
~/Library/Android/sdk/platform-tools/adb
```
rusty-adb checks this path automatically if `adb` isn't on your `$PATH`.

#### Linux

```bash
# Debian / Ubuntu
sudo apt install android-tools-adb

# Arch
sudo pacman -S android-tools

# Fedora
sudo dnf install android-tools
```

#### Windows

1. Download [Android Platform Tools](https://developer.android.com/tools/releases/platform-tools) from Google.
2. Extract the zip to a folder (e.g. `C:\platform-tools`).
3. Add that folder to your system `PATH`.
4. Open a new terminal and verify: `adb version`

---

## Set Up Your Android Device

rusty-adb communicates with your phone over **USB Debugging**. You need to enable it once.

### Step 1 — Enable Developer Options

The Developer Options menu is hidden by default.

| Manufacturer | Path |
|---|---|
| **Stock Android / Google Pixel** | Settings → About phone → tap **Build number** 7 times |
| **Samsung** | Settings → About phone → Software information → tap **Build number** 7 times |
| **OnePlus** | Settings → About device → tap **Build number** 7 times |
| **Xiaomi** | Settings → About phone → tap **MIUI version** 7 times |

You'll see a toast: *"You are now a developer!"*

### Step 2 — Enable USB Debugging

1. Go to **Settings → Developer options** (may be under **System** on some devices).
2. Enable **USB Debugging**.
3. Confirm the prompt if asked.

### Step 3 — Connect via USB

1. Plug your phone into your computer with a USB cable.
2. Pull down the notification shade on the phone — you'll see a **USB connection** notification. Tap it and choose **File Transfer** (MTP) or leave it as **Charging** — rusty-adb works either way.
3. On your phone, a dialog will appear:

   > *Allow USB debugging?*
   > *RSA key fingerprint: ...*

4. Tick **Always allow from this computer**, then tap **Allow**.

> **Not seeing the dialog?** Try: Settings → Developer options → **Revoke USB debugging authorizations**, disconnect, reconnect.

---

## Build and Run

Clone the repo and run the app with a single command:

```bash
git clone https://github.com/aaroncroberts/rusty-adb.git
cd rusty-adb

# Development build + run (auto-reloads on file changes if cargo-watch is installed)
./scripts/dev.sh

# Or directly:
cargo run --package rusty-adb
```

The first build takes a minute or two — Iced and wgpu have large dependency trees. Subsequent builds are fast.

---

## Install a Release Binary

For daily use, build an optimised release binary:

```bash
# Build the release binary
./scripts/build.sh
# Binary is at: ./target/release/rusty-adb

# Install to ~/bin (created if absent)
./scripts/install.sh

# Or install to a custom location
./scripts/install.sh --prefix ~/.local/bin

# Or install with cargo
cargo install --path rusty-adb
```

After install, launch with:

```bash
rusty-adb
```

---

## Connect Your Device

When rusty-adb launches, it:

1. Searches for `adb` on your `$PATH` and at the Android Studio default path.
2. Starts the ADB server (`adb start-server`) in the background.
3. Polls for connected devices every **2 seconds**.

The **status bar** at the bottom shows connection state:

| Status | Meaning |
|---|---|
| `No ADB found` | ADB isn't installed — see [Prerequisites](#2-adb-android-debug-bridge) |
| `Disconnected` | ADB found, no device plugged in |
| `Unauthorized` | Device connected but the *Allow USB Debugging* dialog hasn't been accepted |
| `Connecting…` | ADB handshake in progress |
| `Connected — Model Name` | Ready to go |

> **Unauthorized?** Check your phone screen — the authorization dialog may be waiting.

Once connected, the Android pane on the right loads your device's storage. If your device has an external SD card, storage root chips appear below the toolbar so you can switch between Internal and SD Card.

---

## Your First File Transfer

### Push a file to your Android device

1. Navigate to a folder on your device in the **right (Android) pane**.
2. Navigate to a folder with files in the **left (Local) pane**.
3. Click a file (or `Ctrl`/`⌘`-click to select multiple).
4. Click **Copy to Device** in the local pane toolbar.
5. Confirm in the prompt that appears.

The file appears in the **copy queue** and transfers in the background. Progress is shown in the status bar: `Copying 1/1 · 42%`.

### Pull a file from your Android device

1. Navigate to a file in the **right (Android) pane**.
2. Select it (click once).
3. Click **To Local** in the Android pane toolbar.
4. The file downloads to your current local directory.

---

## Next Steps

Now that you're up and running, explore the rest of the features:

- [File Browser](file-browser.md) — master view modes, sorting, and multi-select
- [File Transfers](file-transfers.md) — learn about the copy queue, drag-and-drop, and cancellation
- [APK & App Management](apk-management.md) — install APKs and manage installed apps
- [Device Details](device-details.md) — explore device information and storage breakdown
- [Settings & Logs](settings.md) — customise logging behaviour

---

← [Documentation Index](README.md)
