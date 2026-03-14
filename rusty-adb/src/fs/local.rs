//! Local filesystem backend for [`FilePane`](crate::file_pane::FilePane).
//!
//! [`LocalFs`] implements [`FileSystem`] using `std::fs`, wrapping blocking
//! calls in `tokio::task::spawn_blocking` so they don't block the async runtime.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{DirEntry, FsError, FileSystem, SortField, format_unix_date};

// ─── Backend Struct ───────────────────────────────────────────────────────────

/// Local filesystem backend. Zero-size — all state lives in [`FileSystem::Context`] (`()`).
#[derive(Debug, Clone)]
pub struct LocalFs;

impl FileSystem for LocalFs {
    /// Local filesystem needs no external context.
    type Context = ();

    async fn list_dir(_ctx: &(), path: &Path) -> Result<Vec<DirEntry>, FsError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || load_local_entries(&path))
            .await
            .map_err(|e| FsError(e.to_string()))?
    }

    /// The local filesystem has no navigation root — the user can always
    /// navigate up to `/` (or the drive root on Windows).
    fn is_nav_root(_ctx: &(), _path: &Path) -> bool {
        false
    }
}

// ─── Directory Loader ─────────────────────────────────────────────────────────

/// Read all entries in `path` and convert to [`DirEntry`] values.
///
/// Hidden-file filtering is handled at render time (like the Android pane),
/// so this function always returns all entries. The caller controls visibility
/// via `FilePane::show_hidden`.
pub fn load_local_entries(path: &PathBuf) -> Result<Vec<DirEntry>, FsError> {
    let read = std::fs::read_dir(path).map_err(|e| FsError(e.to_string()))?;

    let entries: Vec<DirEntry> = read
        .filter_map(|res| res.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_hidden = name.starts_with('.');
            let meta = entry.metadata().ok()?;
            let modified_display = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| format_unix_date(d.as_secs()))
                .unwrap_or_else(|| "--".to_string());
            let is_dir = meta.is_dir();
            let child_count = if is_dir {
                std::fs::read_dir(entry.path()).ok().map(|d| d.count())
            } else {
                None
            };
            Some(DirEntry {
                path: entry.path(),
                is_dir,
                is_symlink: false,
                size: meta.len(),
                modified_display,
                is_hidden,
                name,
                child_count,
            })
        })
        .collect();

    Ok(entries)
}

/// Sort a `Vec<DirEntry>` in-place: directories first, then by `field` / `ascending`.
pub fn sort_entries(
    entries: &mut Vec<DirEntry>,
    field: SortField,
    ascending: bool,
) {
    entries.sort_by(|a, b| {
        match (a.is_dir || a.is_symlink, b.is_dir || b.is_symlink) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let base = match field {
                    SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortField::Size => a.size.cmp(&b.size),
                    SortField::Modified => a.modified_display.cmp(&b.modified_display),
                };
                if ascending { base } else { base.reverse() }
            }
        }
    });
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_local_entries_valid_path() {
        let path = std::env::temp_dir();
        let result = load_local_entries(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn load_local_entries_invalid_path() {
        let path = PathBuf::from("/nonexistent/path/xyz");
        let result = load_local_entries(&path);
        assert!(result.is_err());
    }

    #[test]
    fn dirs_sorted_before_files() {
        use std::fs;
        let tmp = std::env::temp_dir().join("rusty_adb_lfs_sort");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("aaa_file.txt"), b"x").unwrap();
        fs::create_dir_all(tmp.join("zzz_dir")).unwrap();

        let mut entries = load_local_entries(&tmp).unwrap();
        sort_entries(&mut entries, SortField::Name, true);
        assert!(entries[0].is_dir, "first entry should be a directory");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sort_descending_reverses_order() {
        use std::fs;
        let tmp = std::env::temp_dir().join("rusty_adb_lfs_desc");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("aaa.txt"), b"a").unwrap();
        fs::write(tmp.join("zzz.txt"), b"z").unwrap();

        let mut asc = load_local_entries(&tmp).unwrap();
        sort_entries(&mut asc, SortField::Name, true);
        let mut desc = load_local_entries(&tmp).unwrap();
        sort_entries(&mut desc, SortField::Name, false);

        assert_eq!(asc[0].name, "aaa.txt");
        assert_eq!(desc[0].name, "zzz.txt");
        let _ = fs::remove_dir_all(&tmp);
    }
}
