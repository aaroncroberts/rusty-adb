# Settings & Logs

rusty-adb has a settings dialog for adjusting logging behaviour, plus an in-app log viewer for diagnosing issues without leaving the application.

---

## Table of Contents

- [Opening Settings](#opening-settings)
- [Settings Reference](#settings-reference)
  - [Log Level](#log-level)
  - [Console Logging](#console-logging)
  - [File Logging](#file-logging)
- [config.yml Reference](#configyml-reference)
- [In-App Log Viewer](#in-app-log-viewer)
  - [Opening the Log Viewer](#opening-the-log-viewer)
  - [Selecting a Log File](#selecting-a-log-file)
  - [Filtering by Level](#filtering-by-level)
- [Log File Location](#log-file-location)

---

## Opening Settings

Open the Settings dialog in any of these ways:

- Click **⚙ Settings** in the header toolbar.
- Press the **`S`** key anywhere in the app (not while renaming a file).

Close with the **Close** button or press `Escape`.

---

## Settings Reference

### Log Level

Controls the minimum severity of messages that are recorded. Options:

| Level | What gets logged |
|---|---|
| `trace` | Everything — very verbose, hot-path detail. Use only for deep debugging. |
| `debug` | Low-level diagnostics useful for tracing a specific problem. |
| `info` | **Default.** Significant lifecycle events: device connected, transfer started, settings saved. |
| `warn` | Recoverable errors and anything the user should know about. |
| `error` | Unrecoverable failures and data-loss risks. |

For daily use, `info` is recommended. If you're reporting a bug, set this to `debug` or `trace`, reproduce the issue, and attach the log file.

### Console Logging

When enabled, log output is written to **stderr** in addition to the log file. Useful when running rusty-adb from a terminal and you want to watch logs live:

```bash
rusty-adb 2>&1 | tee rusty-adb-debug.log
```

Disabled by default to keep the terminal clean.

### File Logging

When enabled (default), logs are written to rotating files in `~/.rusty-adb/`. Old log files are automatically cleaned up. Disable only if disk space is a concern.

---

## config.yml Reference

Settings are persisted to `~/.rusty-adb/config.yml`. The file is created with defaults on first launch. You can edit it directly in a text editor — changes take effect on the next app launch.

```yaml
log:
  level: info              # trace | debug | info | warn | error
  console_enabled: false   # write log output to stderr
  file_enabled: true       # write rotating log files to ~/.rusty-adb/
```

All fields are optional. Missing fields fall back to the defaults shown above.

**File location by platform:**

| Platform | Path |
|---|---|
| macOS | `~/.rusty-adb/config.yml` |
| Linux | `~/.rusty-adb/config.yml` |
| Windows | `%USERPROFILE%\.rusty-adb\config.yml` |

---

## In-App Log Viewer

The log viewer lets you browse, search, and filter rusty-adb's log files without leaving the app or opening a separate terminal.

### Opening the Log Viewer

Click **Logs** in the bottom-right of the **status bar**:

```
Connected — Pixel 7        3 pending    Logs
                                        ^^^^
                                    click here
```

The log viewer opens as a full-window overlay. Close it with the **Close** button or press `Escape`.

### Selecting a Log File

rusty-adb rotates log files automatically. The viewer shows a list of available log files on the left side, sorted newest-first. Click any file to load it.

The current session's log is always at the top of the list.

### Filtering by Level

A **level filter** dropdown at the top of the log viewer lets you narrow what's shown:

| Filter | Shows |
|---|---|
| `ALL` | Every log line, regardless of level |
| `DEBUG` | DEBUG and above (DEBUG, INFO, WARN, ERROR) |
| `INFO` | INFO and above (INFO, WARN, ERROR) — good default |
| `WARN` | Warnings and errors only |
| `ERROR` | Errors only |

Lines are colour-coded by level:
- **TRACE** / **DEBUG** — muted/secondary colour
- **INFO** — normal text colour
- **WARN** — warning/amber colour
- **ERROR** — red

> **Performance note:** Very large log files (tens of thousands of lines) are capped at the most recent **300 lines** to keep the viewer responsive. If you need to search the full log, open the file directly from `~/.rusty-adb/`.

---

## Log File Location

Log files are written to:

| Platform | Directory |
|---|---|
| macOS | `~/.rusty-adb/` |
| Linux | `~/.rusty-adb/` |
| Windows | `%USERPROFILE%\.rusty-adb\` |

Files are named `rusty-adb.log`, `rusty-adb.log.1`, `rusty-adb.log.2`, etc., rotating automatically when they reach a size limit. The copy queue state (`queue.json`) is also stored in this directory.

---

← [Device Details](device-details.md) · [Documentation Index](README.md)
