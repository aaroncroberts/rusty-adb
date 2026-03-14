//! Pane navigation handlers: local pane and android pane navigation,
//! sorting, selection, RefreshPanes, DisconnectDevice, SpinnerTick.
use crate::adb::AdbOperations;
use crate::fs::{android_entry_to_dir_entry, DirEntry, FileSystem, LocalFs, SortField};
use crate::{App, Message};
use iced::Task;
use std::path::PathBuf;

impl App {
    // ── Local Pane ────────────────────────────────────────────────────────────

    pub(super) fn local_navigate_to(&mut self, path: PathBuf) -> Task<Message> {
        self.local_pane.begin_navigate(path.clone());
        let load_path = path.clone();
        Task::perform(
            async move {
                LocalFs::list_dir(&(), &path)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| match result {
                Ok(entries) => Message::LocalEntriesLoaded {
                    path: load_path.clone(),
                    entries,
                },
                Err(e) => Message::LocalLoadError(e),
            },
        )
    }

    pub(super) fn local_entries_loaded(
        &mut self,
        path: PathBuf,
        entries: Vec<DirEntry>,
    ) -> Task<Message> {
        self.local_pane.on_entries_loaded(path, entries);
        Task::none()
    }

    pub(super) fn local_load_error(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "local directory load error");
        self.local_pane.on_error(msg);
        Task::none()
    }

    pub(super) fn local_select_entry(&mut self, i: usize) -> Task<Message> {
        self.local_pane.select(i);
        Task::none()
    }

    pub(super) fn local_toggle_hidden(&mut self) -> Task<Message> {
        self.local_pane.toggle_hidden();
        // Reload after toggling hidden (shows/hides the dot-files)
        let path = self.local_pane.current_path.clone();
        self.update(Message::LocalNavigateTo(path))
    }

    pub(super) fn local_toggle_type(&mut self) -> Task<Message> {
        self.local_pane.toggle_type();
        Task::none()
    }

    pub(super) fn local_toggle_size(&mut self) -> Task<Message> {
        self.local_pane.toggle_size();
        Task::none()
    }

    pub(super) fn local_toggle_modified(&mut self) -> Task<Message> {
        self.local_pane.toggle_modified();
        Task::none()
    }

    pub(super) fn local_sort_by(&mut self, field: SortField) -> Task<Message> {
        self.local_pane.set_sort(field);
        Task::none()
    }

    pub(super) fn local_navigate_up(&mut self) -> Task<Message> {
        let parent = self
            .local_pane
            .current_path
            .parent()
            .map(|p| p.to_path_buf());
        if let Some(parent) = parent {
            return self.update(Message::LocalNavigateTo(parent));
        }
        Task::none()
    }

    // ── Android Pane ──────────────────────────────────────────────────────────

    pub(super) fn android_toggle_hidden(&mut self) -> Task<Message> {
        self.android_pane.toggle_hidden();
        Task::none()
    }

    pub(super) fn android_toggle_type(&mut self) -> Task<Message> {
        self.android_pane.toggle_type();
        Task::none()
    }

    pub(super) fn android_toggle_size(&mut self) -> Task<Message> {
        self.android_pane.toggle_size();
        Task::none()
    }

    pub(super) fn android_toggle_modified(&mut self) -> Task<Message> {
        self.android_pane.toggle_modified();
        Task::none()
    }

    pub(super) fn android_sort_by(&mut self, field: SortField) -> Task<Message> {
        self.android_pane.set_sort(field);
        Task::none()
    }

    pub(super) fn android_navigate_to(&mut self, path: PathBuf) -> Task<Message> {
        let Some(client) = self.adb_client.clone() else {
            tracing::warn!(path = %path.display(), "android_navigate_to: no adb_client — navigation silently dropped");
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            tracing::warn!(path = %path.display(), "android_navigate_to: no active_serial — navigation silently dropped");
            return Task::none();
        };
        tracing::debug!(path = %path.display(), serial = %serial, "android_navigate_to: starting navigation");

        self.android_pane.begin_navigate(path.clone());

        Task::perform(
            async move { fetch_android_dir(&client, &serial, path).await },
            |result| match result {
                Ok((path, entries, roots)) => Message::AndroidEntriesLoaded {
                    path,
                    entries,
                    roots,
                },
                Err(e) => Message::AndroidLoadError(e),
            },
        )
    }

    pub(super) fn android_entries_loaded(
        &mut self,
        path: PathBuf,
        entries: Vec<DirEntry>,
        roots: Vec<PathBuf>,
    ) -> Task<Message> {
        let first_load = self.android_pane.storage_roots.is_empty();

        // Update storage roots in the context so is_nav_root works correctly
        if let Some(ref mut ctx) = self.android_ctx {
            ctx.storage_roots = roots.clone();
        }
        self.android_pane.storage_roots = roots;
        self.android_pane.on_entries_loaded(path.clone(), entries);

        // On the very first load the app always starts at /sdcard (the legacy
        // symlink). Once we have the real roots, redirect to the preferred one:
        // external storage first, /storage/emulated/0 second, /sdcard as fallback.
        if first_load && path == std::path::Path::new("/sdcard") {
            let preferred = preferred_android_root(&self.android_pane.storage_roots);
            if preferred != path {
                tracing::info!(
                    from = "/sdcard",
                    to   = %preferred.display(),
                    "redirecting to preferred storage root on first connect"
                );
                return self.update(Message::AndroidNavigateTo(preferred));
            }
        }

        Task::none()
    }

    pub(super) fn android_load_error(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "android directory load error");
        self.android_pane.on_error(msg);
        Task::none()
    }

    pub(super) fn android_select_entry(&mut self, i: usize) -> Task<Message> {
        // Double-click detection: same index within 400 ms → preview
        let now = std::time::Instant::now();
        let is_double_click = self
            .android_last_click
            .as_ref()
            .map(|(prev_i, t)| *prev_i == i && t.elapsed().as_millis() < 400)
            .unwrap_or(false);

        if is_double_click {
            self.android_last_click = None;
            if let Some(entry) = self.android_pane.entries.get(i).cloned() {
                if entry.is_navigable() {
                    return self.update(Message::AndroidNavigateTo(entry.path.clone()));
                } else {
                    return self.update(Message::PreviewFile(entry));
                }
            }
        } else {
            self.android_last_click = Some((i, now));
            self.android_pane.select(i);
        }
        Task::none()
    }

    pub(super) fn spinner_tick(&mut self) -> Task<Message> {
        self.android_pane.tick_spinner();
        Task::none()
    }

    // ── Shared navigation ─────────────────────────────────────────────────────

    pub(super) fn refresh_panes(&mut self) -> Task<Message> {
        // Reload local pane via async task
        let local_path = self.local_pane.current_path.clone();
        let local_task = self.update(Message::LocalNavigateTo(local_path));
        // Re-fetch the current android directory
        let android_path = self.android_pane.current_path.clone();
        if self.active_serial.is_some() {
            let android_task = self.update(Message::AndroidNavigateTo(android_path));
            return Task::batch([local_task, android_task]);
        }
        local_task
    }

    pub(super) fn disconnect_device(&mut self) -> Task<Message> {
        let Some(client) = self.adb_client.clone() else {
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };
        tracing::info!(serial = %serial, "user requested disconnect");
        Task::perform(
            async move { client.disconnect(&serial).await.map_err(|e| e.to_string()) },
            |result| {
                if let Err(e) = result {
                    tracing::warn!(error = %e, "disconnect request failed, polling will reflect new state");
                }
                Message::PollDevices
            },
        )
    }
}

// ─── Extracted async helpers (testable without constructing App) ──────────────

/// Fetch a directory listing and storage roots from an Android device.
///
/// This is the core async work inside `android_navigate_to` extracted so it
/// can be exercised in unit tests via [`crate::adb::mock::MockAdbClient`].
pub(crate) async fn fetch_android_dir(
    client: &impl AdbOperations,
    serial: &str,
    path: PathBuf,
) -> Result<(PathBuf, Vec<DirEntry>, Vec<PathBuf>), String> {
    let entries_fut = client.list_dir(serial, &path);
    let roots_fut = client.list_storage_roots(serial);
    let (entries_res, roots) = tokio::join!(entries_fut, roots_fut);
    entries_res
        .map(|raw| {
            let entries: Vec<DirEntry> = raw.into_iter().map(android_entry_to_dir_entry).collect();
            (path, entries, roots)
        })
        .map_err(|e| e.to_string())
}

/// Choose the best initial root to display after a device connects.
///
/// Preference order:
/// 1. External storage (`/storage/<name>` where name ≠ `emulated` and ≠ `self`)
/// 2. Internal canonical path (`/storage/emulated/0`)
/// 3. Legacy symlink (`/sdcard`) — fallback when nothing better is available
pub(crate) fn preferred_android_root(roots: &[std::path::PathBuf]) -> std::path::PathBuf {
    // External: any /storage/* root that isn't emulated or the sdcard symlink
    let external = roots.iter().find(|r| {
        let s = r.to_string_lossy();
        s.starts_with("/storage/")
            && !s.starts_with("/storage/emulated/")
            && *r != std::path::Path::new("/sdcard")
    });
    if let Some(ext) = external {
        return ext.clone();
    }

    // Internal canonical: /storage/emulated/0 or any /storage/emulated/N
    let emulated = roots
        .iter()
        .find(|r| r.to_string_lossy().starts_with("/storage/emulated/"));
    if let Some(emu) = emulated {
        return emu.clone();
    }

    // Fallback
    std::path::PathBuf::from("/sdcard")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::mock::MockAdbClient;
    use crate::adb::parser::AndroidEntry;
    use std::path::PathBuf;

    fn make_entry(name: &str, is_dir: bool) -> AndroidEntry {
        AndroidEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/sdcard/{name}")),
            size: 0,
            modified: "2024-01-01".to_string(),
            is_dir,
            is_symlink: false,
            is_hidden: false,
        }
    }

    // ── fetch_android_dir ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn fetch_android_dir_returns_entries_and_roots() {
        let mut mock = MockAdbClient::default();
        mock.entries = vec![
            make_entry("DCIM", true),
            make_entry("Download", true),
            make_entry("photo.jpg", false),
        ];
        mock.roots = vec![
            PathBuf::from("/sdcard"),
            PathBuf::from("/storage/emulated/0"),
        ];

        let (path, entries, roots) =
            fetch_android_dir(&mock, "serial123", PathBuf::from("/sdcard"))
                .await
                .expect("should succeed");

        assert_eq!(path, PathBuf::from("/sdcard"));
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "DCIM");
        assert!(entries[0].is_dir);
        assert_eq!(entries[2].name, "photo.jpg");
        assert!(!entries[2].is_dir);
        assert_eq!(roots.len(), 2);

        let calls = mock.calls();
        assert!(calls.iter().any(|c| c.contains("list_dir")));
        assert!(calls.iter().any(|c| c.contains("list_storage_roots")));
    }

    #[tokio::test]
    async fn fetch_android_dir_list_dir_error_propagates() {
        let mock = MockAdbClient::failing_list_dir();

        let result = fetch_android_dir(&mock, "serial123", PathBuf::from("/sdcard")).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("mock list_dir failure"), "got: {err}");
    }

    #[tokio::test]
    async fn fetch_android_dir_empty_directory() {
        let mock = MockAdbClient::default(); // entries = []
        let (_, entries, roots) = fetch_android_dir(&mock, "device1", PathBuf::from("/sdcard"))
            .await
            .unwrap();

        assert!(entries.is_empty());
        assert!(!roots.is_empty()); // always at least /sdcard from default mock
    }

    #[tokio::test]
    async fn fetch_android_dir_records_serial_in_calls() {
        let mock = MockAdbClient::default();
        let _ = fetch_android_dir(&mock, "R5CWA0X", PathBuf::from("/sdcard")).await;

        let calls = mock.calls();
        assert!(
            calls.iter().any(|c| c.contains("R5CWA0X")),
            "serial should appear in call log: {calls:?}"
        );
    }

    // ── preferred_android_root ────────────────────────────────────────────────

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn prefers_external_over_emulated() {
        let roots = vec![
            p("/sdcard"),
            p("/storage/emulated/0"),
            p("/storage/external_sd"),
        ];
        assert_eq!(preferred_android_root(&roots), p("/storage/external_sd"));
    }

    #[test]
    fn falls_back_to_emulated_when_no_external() {
        let roots = vec![p("/sdcard"), p("/storage/emulated/0")];
        assert_eq!(preferred_android_root(&roots), p("/storage/emulated/0"));
    }

    #[test]
    fn falls_back_to_sdcard_when_only_root() {
        let roots = vec![p("/sdcard")];
        assert_eq!(preferred_android_root(&roots), p("/sdcard"));
    }

    #[test]
    fn empty_roots_returns_sdcard_fallback() {
        assert_eq!(preferred_android_root(&[]), p("/sdcard"));
    }
}
