# File Browser

The file browser is the heart of rusty-adb — a two-pane layout that shows your local computer on the left and your Android device on the right.

---

## Table of Contents

- [Layout Overview](#layout-overview)
- [View Modes](#view-modes)
- [The Show Panel](#the-show-panel)
- [Sorting](#sorting)
- [Selecting Files](#selecting-files)
- [Navigating Directories](#navigating-directories)
- [Storage Roots (Android)](#storage-roots-android)
- [Expanding a Pane](#expanding-a-pane)
- [Keyboard Shortcuts](#keyboard-shortcuts)

---

## Layout Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  ⚙ Settings   ☰ Queue          ◄◄ RUSTY-ADB ►► v0.1.0         │  ← Header
├───────────────────────┬─────────────────────────────────────────┤
│  LOCAL  ~/Documents   │  ANDROID  /storage/emulated/0           │  ← Title bars
│  View [As List ▼] [⊞] │  View [As List ▼] [APK] [To Local] [⊞]  │  ← Toolbars
├───────────────────────┼─────────────────────────────────────────┤
│  file.txt      4 KB   │  DCIM/         Folder                   │
│  photo.jpg   120 KB   │  Download/     Folder                   │
│  project/    Folder   │  Documents/    Folder                   │
│  ...                  │  ...                                    │
├───────────────────────┴─────────────────────────────────────────┤
│  Connected — Pixel 7        Copying 1/3 · 42%    Logs           │  ← Status bar
└─────────────────────────────────────────────────────────────────┘
```

**Left pane** — your local filesystem. Starts at your home directory (`~`).
**Right pane** — your Android device. Starts at the device's primary storage. Shows *"No Android Device Connected"* with setup instructions when no device is plugged in.

---

## View Modes

Each pane has an independent **View** dropdown in its toolbar. Four modes are available:

### List
Single-column rows showing just the filename and a folder/file icon. The highest-density mode — fits the most entries on screen at once.

```
📁  Applications
📁  Desktop
📄  notes.txt
📄  report.pdf
```

### Details
Tabular rows with columns: **Name**, **Type**, **Size**, **Modified**. Columns can be individually toggled via the [Show panel](#the-show-panel). This is the best mode for comparing file sizes or dates.

```
Name              Type     Size      Modified
────────────────────────────────────────────
📁 Applications   Folder   —         2026-01-15
📄 notes.txt      txt      4 KB      2026-03-01
📄 report.pdf     pdf      120 KB    2026-02-28
```

### Grid
Compact multi-column tiles (4 per row). Good for directories with many small files.

### Icon
Large tiles (2–3 per row) with a prominent type icon. Best for image-heavy folders or when you want a quick visual scan.

> **Tip:** View mode is remembered per pane independently. You can have List on the left and Details on the right at the same time.

---

## The Show Panel

Click the **⊞** icon at the right of any pane's toolbar to open the **Show panel** — a collapsible row of toggles beneath the toolbar.

| Toggle | Effect |
|---|---|
| **Type** | Show/hide the Type column (Details mode) |
| **Size** | Show/hide the Size column (Details mode) |
| **Modified** | Show/hide the Modified date column (Details mode) |
| **Hidden files** | Show/hide dotfiles (`.filename`) |

Toggles with a `✓` prefix are currently enabled. Click again to disable. Settings are per-pane and persist across sessions.

---

## Sorting

In **Details mode**, click any column header to sort by that column. Click the same header again to reverse the sort order. An arrow indicator shows the active sort direction.

| Column | Sorts by |
|---|---|
| Name | Filename, alphabetically |
| Type | File extension |
| Size | File size in bytes |
| Modified | Last modification timestamp |

Folders always sort before files, regardless of the sort column.

---

## Selecting Files

### Single selection
Click any entry to select it. The selected entry is highlighted.

### Multi-selection
- **Click + `Ctrl`** (or `⌘` on macOS) — add or remove individual entries from the selection.
- **Click + `Shift`** — select a contiguous range from the last-clicked entry to the current one.

Selected entries are highlighted. The toolbar's action buttons (Copy to Device, To Local, Delete, etc.) apply to all selected entries at once.

### Deselect
Click empty space in the pane, or click a selected entry without modifier keys to reduce to a single selection.

---

## Navigating Directories

### Opening a folder
**Double-click** any folder to navigate into it.

### Going up
- **Backspace** — navigate up one level in the **local** pane.
- Click the **`..`** entry at the top of the list (if present).
- Click any segment in the **breadcrumb bar** in the pane title.

### Breadcrumb navigation
The pane title bar shows the current path as a series of clickable segments:

```
ANDROID  /storage / emulated / 0 / DCIM / Camera
```

Click any ancestor segment to jump directly to that directory.

### Refresh
Press **F5** (or use the toolbar) to reload both panes from disk/device.

---

## Storage Roots (Android)

If your Android device has an external SD card or multiple storage volumes, **storage root chips** appear in the Android pane toolbar:

```
View [As List ▼]  [ Internal ]  [ SD Card ]  [⊞]
```

Click a chip to navigate to that storage root. The active root is highlighted in accent color. rusty-adb resolves `/sdcard` and `/storage/emulated/0` as the same location automatically.

---

## Expanding a Pane

When you want more space to browse a single pane, use the **expand** control.

- Click the **expand icon** (⤢) in a pane's title bar to expand that pane to full width. The other pane collapses, and a **sidebar** appears with restore/split controls.
- Click **Restore split** in the sidebar (or the icon next to the expanded pane's title) to return to the two-pane view.

This is especially useful when browsing a deeply nested directory tree.

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `F5` | Refresh both panes |
| `F2` | Rename the selected Android file |
| `Backspace` | Navigate up in the local pane |
| `S` | Open Settings |
| `Escape` | Close the topmost modal or cancel a rename |
| `Double-click` | Open a folder / preview a file |
| `Ctrl/⌘ + click` | Add/remove from multi-selection |
| `Shift + click` | Range-select entries |

---

← [Documentation Index](README.md) · [File Transfers →](file-transfers.md)
