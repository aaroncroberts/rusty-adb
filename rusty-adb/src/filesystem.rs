//! Shared filesystem abstractions used by both the local and Android panes.
//!
//! # Overview
//!
//! This module defines the types that the unified [`FilePane`](crate::file_pane::FilePane)
//! uses regardless of whether it is browsing the local disk or an Android device:
//!
//! - [`DirEntry`]    — one normalized directory entry (file, folder, or symlink)
//! - [`PaneState`]   — four-state machine shared by both pane kinds
//! - [`SortField`]   — which column to sort by
//! - [`FsError`]     — filesystem operation error
//! - [`FileSystem`]  — the trait both backends implement
//! - Formatting helpers: [`format_size`], [`format_unix_date`]

use std::path::{Path, PathBuf};

// ─── Unified Entry Type ───────────────────────────────────────────────────────

/// A single normalized directory entry produced by any [`FileSystem`] backend.
///
/// Both the local filesystem and the Android ADB backend map their native
/// entry types into this common representation so the shared view code in
/// `FilePane` can render them identically.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    /// Pre-formatted `"YYYY-MM-DD"` date, or `"--"` when unavailable.
    pub modified_display: String,
    pub is_dir: bool,
    /// True for Android symlinks (e.g. `/sdcard → /storage/emulated/0`).
    /// Always `false` for local filesystem entries.
    pub is_symlink: bool,
    pub is_hidden: bool,
}

impl DirEntry {
    /// Human-readable file size: directories and symlinks show `"--"`,
    /// files show KB / MB / GB.
    pub fn size_display(&self) -> String {
        if self.is_dir {
            "--".to_string()
        } else {
            format_size(self.size)
        }
    }

    /// `true` when this entry can be navigated into (directory or symlink-to-dir).
    pub fn is_navigable(&self) -> bool {
        self.is_dir || self.is_symlink
    }

    /// Short type label for the **Type** column.
    pub fn type_label(&self) -> &str {
        if self.is_dir {
            "Folder"
        } else if self.is_symlink {
            "Symlink"
        } else {
            self.name
                .rsplit('.')
                .next()
                .filter(|ext| !ext.is_empty() && *ext != self.name.as_str())
                .unwrap_or("File")
        }
    }

    /// ASCII icon glyph used in list / detail views.
    pub fn icon(&self) -> &'static str {
        if self.is_dir {
            "[/]"
        } else if self.is_symlink {
            "[@]"
        } else {
            "[-]"
        }
    }
}

// ─── Sort Field ───────────────────────────────────────────────────────────────

/// Which column entries are sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    Size,
    Modified,
}

// ─── Pane State ───────────────────────────────────────────────────────────────

/// Lifecycle state of a file browser pane.
///
/// The local pane only ever uses [`Ready`](PaneState::Ready) and
/// [`Error`](PaneState::Error). The Android pane uses all four variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneState {
    /// No authorized device connected *(Android pane only)*.
    NoDevice,
    /// Async directory fetch in progress *(Android pane only)*.
    Loading,
    /// Entries are ready to display.
    Ready,
    /// Last operation failed; the string is the human-readable error.
    Error(String),
}

// ─── Filesystem Trait ─────────────────────────────────────────────────────────

/// Abstraction over local and Android directory access.
///
/// # Design notes
/// - `list_dir` is async to accommodate ADB's subprocess model; the local
///   implementation wraps `std::fs` in `tokio::task::spawn_blocking`.
/// - The associated `Context` type carries per-operation state (e.g. ADB
///   client + device serial). The local filesystem uses `()` as its context.
/// - The trait is intentionally **not** object-safe (`impl Future` return
///   types). `FilePane<LocalFs>` and `FilePane<AndroidFs>` are two distinct
///   monomorphized types with zero vtable overhead.
pub trait FileSystem: Clone + std::fmt::Debug + Send + 'static {
    /// Per-operation context. Local filesystem uses `()`.
    type Context: Clone + std::fmt::Debug + Send + Sync + 'static;

    /// List a directory, returning normalized [`DirEntry`] values.
    fn list_dir(
        ctx: &Self::Context,
        path: &Path,
    ) -> impl std::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send;

    /// Returns `true` if `path` is a navigation root where the `".."` button
    /// should be hidden. Always `false` for the local filesystem.
    fn is_nav_root(ctx: &Self::Context, path: &Path) -> bool;
}

// ─── Error Type ───────────────────────────────────────────────────────────────

/// A filesystem operation error (wraps a human-readable message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError(pub String);

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for FsError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FsError {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ─── Formatting Helpers ────────────────────────────────────────────────────────

/// Format a byte count into a human-readable string (e.g. `"1.4 MB"`).
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Convert a Unix timestamp (seconds since epoch) to a `"YYYY-MM-DD"` string (UTC).
pub fn format_unix_date(unix_secs: u64) -> String {
    let days_since_epoch = unix_secs / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch as i64);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Convert days since 1970-01-01 to `(year, month, day)` in the proleptic
/// Gregorian calendar.
///
/// Algorithm from <http://howardhinnant.github.io/date_algorithms.html>.
pub fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn format_size_kb() {
        assert_eq!(format_size(1024), "1 KB");
        assert_eq!(format_size(1536), "2 KB");
    }

    #[test]
    fn format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn format_date_epoch() {
        assert_eq!(format_unix_date(0), "1970-01-01");
    }

    #[test]
    fn format_date_known() {
        // 2024-01-15 00:00:00 UTC = 1705276800
        assert_eq!(format_unix_date(1705276800), "2024-01-15");
    }

    #[test]
    fn dir_entry_size_display_dir() {
        let e = DirEntry {
            name: "foo".into(),
            path: PathBuf::from("/tmp/foo"),
            size: 4096,
            modified_display: "--".into(),
            is_dir: true,
            is_symlink: false,
            is_hidden: false,
        };
        assert_eq!(e.size_display(), "--");
    }

    #[test]
    fn dir_entry_size_display_file() {
        let e = DirEntry {
            name: "bar.txt".into(),
            path: PathBuf::from("/tmp/bar.txt"),
            size: 2048,
            modified_display: "2024-01-15".into(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
        };
        assert_eq!(e.size_display(), "2 KB");
    }

    #[test]
    fn dir_entry_navigable_dir() {
        let e = DirEntry {
            name: "d".into(),
            path: PathBuf::from("/d"),
            size: 0,
            modified_display: "--".into(),
            is_dir: true,
            is_symlink: false,
            is_hidden: false,
        };
        assert!(e.is_navigable());
        assert_eq!(e.icon(), "[/]");
        assert_eq!(e.type_label(), "Folder");
    }

    #[test]
    fn dir_entry_navigable_symlink() {
        let e = DirEntry {
            name: "sdcard".into(),
            path: PathBuf::from("/sdcard"),
            size: 0,
            modified_display: "--".into(),
            is_dir: false,
            is_symlink: true,
            is_hidden: false,
        };
        assert!(e.is_navigable());
        assert_eq!(e.icon(), "[@]");
        assert_eq!(e.type_label(), "Symlink");
    }

    #[test]
    fn dir_entry_not_navigable_file() {
        let e = DirEntry {
            name: "file.txt".into(),
            path: PathBuf::from("/file.txt"),
            size: 0,
            modified_display: "--".into(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
        };
        assert!(!e.is_navigable());
        assert_eq!(e.icon(), "[-]");
    }
}
