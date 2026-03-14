//! Android ADB filesystem backend for [`FilePane`](crate::file_pane::FilePane).
//!
//! [`AndroidFs`] implements [`FileSystem`] by delegating to [`AdbClient`].
//! The [`AndroidContext`] carries the ADB client, device serial, storage roots,
//! and device label — everything needed to scope a filesystem operation to one
//! specific connected Android device.

use std::path::{Path, PathBuf};

use crate::adb::{AdbClient, AndroidEntry};
use crate::filesystem::{DirEntry, FsError, FileSystem};

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

    fn is_nav_root(ctx: &AndroidContext, path: &Path) -> bool {
        path == Path::new("/") || ctx.storage_roots.contains(&path.to_path_buf())
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
