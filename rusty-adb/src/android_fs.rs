//! Android ADB filesystem backend for [`FilePane`](crate::file_pane::FilePane).
//!
//! [`AndroidFs`] implements [`FileSystem`] by delegating to [`AdbClient`].
//! The [`AndroidContext`] carries the ADB client, device serial, storage roots,
//! and device label — everything needed to scope a filesystem operation to one
//! specific connected Android device.

use std::path::{Path, PathBuf};

use crate::adb::{AdbClient, AndroidEntry};
use crate::filesystem::{DirEntry, FsError, FileSystem, format_unix_date};

// ─── Context ──────────────────────────────────────────────────────────────────

/// Runtime context for the Android filesystem backend.
///
/// Populated when a device connects; cleared on disconnect. Passed by reference
/// to every [`FileSystem`] method call so the backend can reach the ADB daemon.
#[derive(Debug, Clone)]
pub struct AndroidContext {
    /// ADB client (wraps the adb binary path).
    pub client: AdbClient,
    /// Device serial number (passed to every `adb -s <serial>` invocation).
    pub serial: String,
    /// Storage roots discovered on the device (e.g. `/sdcard`, `/storage/…`).
    /// Populated after the first successful `list_dir` call.
    pub storage_roots: Vec<PathBuf>,
    /// Human-readable device label shown in the pane header (e.g. `"Pixel 7"`).
    pub device_label: String,
}

// ─── Backend Struct ───────────────────────────────────────────────────────────

/// Android ADB filesystem backend. Zero-size — all state lives in [`AndroidContext`].
#[derive(Debug, Clone)]
pub struct AndroidFs;

impl FileSystem for AndroidFs {
    type Context = AndroidContext;

    async fn list_dir(ctx: &AndroidContext, path: &Path) -> Result<Vec<DirEntry>, FsError> {
        ctx.client
            .list_dir(&ctx.serial, path)
            .await
            .map(|entries| entries.into_iter().map(android_entry_to_dir_entry).collect())
            .map_err(|e| FsError(e.to_string()))
    }

    async fn rename(ctx: &AndroidContext, from: &Path, to: &Path) -> Result<(), FsError> {
        ctx.client
            .rename(&ctx.serial, from, to)
            .await
            .map_err(|e| FsError(e.to_string()))
    }

    async fn delete(ctx: &AndroidContext, path: &Path) -> Result<(), FsError> {
        ctx.client
            .delete(&ctx.serial, path)
            .await
            .map_err(|e| FsError(e.to_string()))
    }

    fn is_nav_root(ctx: &AndroidContext, path: &Path) -> bool {
        path == Path::new("/") || ctx.storage_roots.contains(&path.to_path_buf())
    }

    fn pane_label(ctx: &AndroidContext) -> String {
        if ctx.device_label.is_empty() {
            "Android Device".to_string()
        } else {
            ctx.device_label.clone()
        }
    }
}

// ─── Entry Conversion ─────────────────────────────────────────────────────────

/// Convert an [`AndroidEntry`] (raw ADB ls parser output) to a [`DirEntry`].
///
/// The `modified` field is already a `"YYYY-MM-DD"` string in `AndroidEntry`,
/// so no further formatting is needed.
pub fn android_entry_to_dir_entry(e: AndroidEntry) -> DirEntry {
    DirEntry {
        name: e.name,
        path: e.path,
        size: e.size,
        modified_display: e.modified,
        is_dir: e.is_dir,
        is_symlink: e.is_symlink,
        is_hidden: e.is_hidden,
    }
}

/// Convert a [`DirEntry`] back to a minimal [`AndroidEntry`] for callers that
/// still need the old type (e.g. file preview). Remove once those callers
/// are migrated to use [`DirEntry`] directly.
pub fn dir_entry_to_android_entry(e: &DirEntry) -> AndroidEntry {
    AndroidEntry {
        name: e.name.clone(),
        path: e.path.clone(),
        size: e.size,
        modified: e.modified_display.clone(),
        is_dir: e.is_dir,
        is_symlink: e.is_symlink,
        is_hidden: e.is_hidden,
    }
}

// suppress unused warning — format_unix_date is used transitively; keep the
// import so the module is self-contained once callers are wired up.
#[allow(dead_code)]
fn _use_format_unix_date(secs: u64) -> String {
    format_unix_date(secs)
}
