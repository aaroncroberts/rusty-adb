# File Transfers

rusty-adb supports transferring files in both directions — pushing to your Android device and pulling to your computer. A persistent **copy queue** handles batching, progress tracking, and recovery across app restarts.

---

## Table of Contents

- [Push Files to Your Device](#push-files-to-your-device)
- [Pull Files from Your Device](#pull-files-from-your-device)
- [Transfer Progress](#transfer-progress)
- [Copy Queue](#copy-queue)
  - [Opening the Queue Dialog](#opening-the-queue-dialog)
  - [Queue Item States](#queue-item-states)
  - [Pause and Resume](#pause-and-resume)
  - [Clear Completed](#clear-completed)
  - [Persistence](#persistence)
- [Drag and Drop](#drag-and-drop)
- [Cancelling a Transfer](#cancelling-a-transfer)
- [Transfer Failures](#transfer-failures)

---

## Push Files to Your Device

To copy files from your computer to your Android device:

1. In the **left (Local) pane**, navigate to the folder containing the files you want to transfer.
2. Select the files or folders you want to send. Use `Ctrl/⌘ + click` for multiple items, or `Shift + click` for a range.
3. Click **Copy to Device** in the local pane toolbar.
4. A confirmation prompt shows the destination path on the device. Confirm to queue the transfer.

The selected files are added to the **copy queue** and begin transferring immediately (unless the queue is paused).

> **Tip:** You can push entire folders — rusty-adb will recursively copy all contents.

---

## Pull Files from Your Device

To copy files from your Android device to your computer:

1. In the **right (Android) pane**, navigate to the file or folder you want to download.
2. Select one or more entries.
3. Click **To Local** in the Android pane toolbar.

The files download immediately to your current local directory. Pull transfers bypass the copy queue and run directly.

---

## Transfer Progress

While a transfer is active, the **status bar** at the bottom of the app switches from showing the connection status to a live progress view:

```
[████████░░░░░░░░░] report.pdf  62%  1.4 MB/s    X Cancel
```

The progress bar shows:
- **Filename** being transferred
- **Percent complete** (0–100%)
- **Transfer speed** (updated in real time)
- **Job position** when multiple items are queued: `[2/5]  photo.jpg  34%`

---

## Copy Queue

The copy queue is a persistent background transfer system. All **push** operations go through it, letting you queue up many transfers and have them run one after another without blocking the UI.

### Opening the Queue Dialog

Open the queue dialog in any of these ways:

- Click **☰ Queue** in the header toolbar.
- Click the queue status text in the status bar (e.g. `3 pending`).

The queue dialog shows all items — pending, active, paused, completed, and failed — along with their current status and destination path.

### Queue Item States

| State | Meaning |
|---|---|
| **Pending** | Waiting to start |
| **Copying** | Actively transferring (shows percent) |
| **Paused** | Queue is paused; this item will go next when resumed |
| **Done** | Transfer completed successfully |
| **Failed** | Transfer failed (reason shown) |

### Pause and Resume

Click **Pause** in the queue dialog to halt processing after the current transfer completes. The queue remembers its position — all pending items stay queued. Click **Resume** to continue.

The pause state is visible in the status bar: `Queue paused · 3 pending`.

### Clear Completed

Click **Clear completed** in the queue dialog to remove all **Done** and **Failed** items from the list. Pending and active items are unaffected.

### Persistence

The copy queue is saved to `~/.rusty-adb/queue.json` after every change. If rusty-adb is closed (or crashes) while transfers are pending, the queue is fully restored on the next launch and transfers resume automatically.

> **Note:** If the app is closed mid-transfer, the interrupted item is reset to **Pending** and retried from the beginning on next launch (ADB `push` does not support resuming partial transfers).

---

## Drag and Drop

You can drag files directly from **Finder** (macOS), **Explorer** (Windows), or your file manager (Linux) onto the rusty-adb window to push them to the current Android directory.

1. Navigate to the destination folder in the **Android pane**.
2. Drag one or more files from your file manager and drop them anywhere on the rusty-adb window.
3. The files are added to the copy queue targeting the current Android directory.

A **drop highlight** shows when the app is ready to accept a drop.

> **Tip:** Drag-and-drop always targets the **current Android directory** shown in the right pane — make sure you've navigated there first.

---

## Cancelling a Transfer

While a transfer is in progress, a **X Cancel** button appears in the status bar next to the progress indicator.

Click it to cancel the active transfer. The current file is abandoned (no partial files are left on the device — ADB cleans up automatically), and the rest of the queue is also cleared.

---

## Transfer Failures

If a transfer fails (e.g. the device disconnects mid-transfer, permissions denied, disk full), the status bar shows a red **error banner** with the reason. The queue item is marked **Failed** and the remaining queue is paused so you can review the situation before retrying.

To retry failed items, open the **Queue dialog**, review the failed item's reason, correct any issue, and click **Resume**.

---

← [File Browser](file-browser.md) · [Documentation Index](README.md) · [APK & App Management →](apk-management.md)
