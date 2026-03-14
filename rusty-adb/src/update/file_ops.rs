//! File operation handlers: rename, delete, and preview.
use crate::adb::AdbOperations;
use crate::file_pane;
use crate::{App, Message, PreviewContent};
use iced::widget::text_input;
use iced::Task;
use std::path::PathBuf;

impl App {
    // ── Rename ────────────────────────────────────────────────────────────────

    pub(super) fn android_begin_rename(&mut self) -> Task<Message> {
        // Activate rename for the first selected entry
        if let Some(&idx) = self.android_pane.selected.first() {
            self.android_pane.begin_rename(idx);
            return text_input::focus(text_input::Id::new(file_pane::RENAME_INPUT_ID));
        }
        Task::none()
    }

    pub(super) fn android_rename_input(&mut self, val: String) -> Task<Message> {
        self.android_pane.update_rename_input(val);
        Task::none()
    }

    pub(super) fn android_rename_cancel(&mut self) -> Task<Message> {
        self.android_pane.cancel_rename();
        Task::none()
    }

    pub(super) fn android_rename_commit(&mut self) -> Task<Message> {
        let Some((idx, new_name)) = self.android_pane.rename_pending.clone() else {
            return Task::none();
        };
        let Some(entry) = self.android_pane.entries.get(idx).cloned() else {
            return Task::none();
        };
        let new_name = new_name.trim().to_string();
        if new_name.is_empty() || new_name == entry.name {
            self.android_pane.cancel_rename();
            return Task::none();
        }
        let Some(client) = &self.adb_client else {
            return Task::none();
        };
        let Some(serial) = &self.active_serial else {
            return Task::none();
        };
        let to_path = entry.path.parent().unwrap_or(&entry.path).join(&new_name);
        let client = client.clone();
        let serial = serial.clone();
        let from_path = entry.path.clone();
        self.android_pane.cancel_rename();
        tracing::info!(
            from = %from_path.display(),
            to = %to_path.display(),
            "renaming android file"
        );
        Task::perform(
            async move { do_rename(&client, &serial, from_path, to_path).await },
            |result| match result {
                Ok(()) => Message::AndroidRenameComplete,
                Err(e) => Message::AndroidRenameFailed(e),
            },
        )
    }

    pub(super) fn android_rename_complete(&mut self) -> Task<Message> {
        tracing::info!("rename complete — refreshing android listing");
        self.update(Message::AndroidNavigateTo(
            self.android_pane.current_path.clone(),
        ))
    }

    pub(super) fn android_rename_failed(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "android rename failed");
        self.update(Message::ShowError(format!("Rename failed: {msg}")))
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    pub(super) fn android_begin_delete(&mut self) -> Task<Message> {
        if self.android_pane.selected.is_empty() {
            return Task::none();
        }
        let paths: Vec<PathBuf> = self
            .android_pane
            .selected
            .iter()
            .filter_map(|&i| self.android_pane.entries.get(i))
            .map(|e| e.path.clone())
            .collect();
        if paths.is_empty() {
            return Task::none();
        }
        self.delete_confirm_paths = Some(paths);
        Task::none()
    }

    pub(super) fn android_delete_cancel(&mut self) -> Task<Message> {
        self.delete_confirm_paths = None;
        Task::none()
    }

    pub(super) fn android_delete_confirm(&mut self) -> Task<Message> {
        let paths = match self.delete_confirm_paths.take() {
            Some(p) => p,
            None => return Task::none(),
        };
        let Some(client) = &self.adb_client else {
            return Task::none();
        };
        let Some(serial) = &self.active_serial else {
            return Task::none();
        };
        let client = client.clone();
        let serial = serial.clone();
        tracing::info!(count = paths.len(), "deleting android files");
        Task::perform(
            async move { do_delete_all(&client, &serial, paths).await },
            |result| match result {
                Ok(()) => Message::AndroidDeleteComplete,
                Err(e) => Message::AndroidDeleteFailed(e),
            },
        )
    }

    pub(super) fn android_delete_complete(&mut self) -> Task<Message> {
        tracing::info!("delete complete — refreshing android listing");
        self.android_pane.selected.clear();
        self.update(Message::AndroidNavigateTo(
            self.android_pane.current_path.clone(),
        ))
    }

    pub(super) fn android_delete_failed(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "android delete failed");
        self.update(Message::ShowError(format!("Delete failed: {msg}")))
    }

    // ── Preview ───────────────────────────────────────────────────────────────

    pub(super) fn preview_file(&mut self, entry: crate::fs::DirEntry) -> Task<Message> {
        const MAX_BYTES: u64 = 10 * 1024 * 1024; // 10 MB
        if entry.size > MAX_BYTES {
            return self.update(Message::ShowError(
                "File too large to preview (max 10 MB)".to_string(),
            ));
        }
        let (Some(client), Some(serial)) = (&self.adb_client, &self.active_serial) else {
            return Task::none();
        };
        let client = client.clone();
        let serial = serial.clone();
        let path = entry.path.clone();
        tracing::info!(file = %path.display(), "pulling file to temp for preview");
        Task::perform(
            async move {
                client
                    .pull_to_temp(&serial, &path)
                    .await
                    .map_err(|e| e.to_string())
            },
            |result| match result {
                Ok(local) => Message::PreviewReady(local),
                Err(e) => Message::PreviewFailed(e),
            },
        )
    }

    pub(super) fn preview_ready(&mut self, local_path: PathBuf) -> Task<Message> {
        self.preview_modal = Some(resolve_preview_content(local_path));
        Task::none()
    }

    pub(super) fn preview_failed(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "file preview pull failed");
        self.update(Message::ShowError(format!("Preview failed: {msg}")))
    }

    pub(super) fn close_preview(&mut self) -> Task<Message> {
        self.preview_modal = None;
        Task::none()
    }
}

// ─── Extracted async helpers (testable without constructing App) ──────────────

/// Rename a file on the device.
///
/// Pure async helper extracted from `android_rename_commit` so it can be
/// tested with a [`crate::adb::mock::MockAdbClient`].
pub(crate) async fn do_rename(
    client: &impl AdbOperations,
    serial: &str,
    from: std::path::PathBuf,
    to: std::path::PathBuf,
) -> Result<(), String> {
    client.rename(serial, &from, &to).await.map_err(|e| e.to_string())
}

/// Delete one or more files on the device sequentially.
///
/// Returns the path of the first failure, if any.
pub(crate) async fn do_delete_all(
    client: &impl AdbOperations,
    serial: &str,
    paths: Vec<std::path::PathBuf>,
) -> Result<(), String> {
    for path in &paths {
        client.delete(serial, path).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Determine the [`crate::PreviewContent`] for a locally-pulled file.
///
/// This is the pure dispatch logic from `preview_ready` — no I/O, just
/// extension matching and an optional `std::fs::read_to_string`.
pub(crate) fn resolve_preview_content(local_path: std::path::PathBuf) -> PreviewContent {
    let ext = local_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" => PreviewContent::Image(local_path),
        "txt" | "log" | "json" | "xml" | "md" | "toml" | "yaml" | "yml" => {
            match std::fs::read_to_string(&local_path) {
                Ok(text) => PreviewContent::Text(text),
                Err(e) => PreviewContent::Unsupported(format!("Could not read file: {e}")),
            }
        }
        other => PreviewContent::Unsupported(format!("Preview not available for .{other} files")),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::mock::MockAdbClient;
    use std::path::PathBuf;

    // ── do_rename ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn do_rename_succeeds() {
        let mock = MockAdbClient::default();
        let result = do_rename(
            &mock,
            "serial1",
            PathBuf::from("/sdcard/old.txt"),
            PathBuf::from("/sdcard/new.txt"),
        )
        .await;
        assert!(result.is_ok());

        let calls = mock.calls();
        assert!(calls.iter().any(|c| c.contains("rename")));
        assert!(calls.iter().any(|c| c.contains("old.txt")));
        assert!(calls.iter().any(|c| c.contains("new.txt")));
    }

    #[tokio::test]
    async fn do_rename_propagates_error() {
        let mock = MockAdbClient::failing_rename();
        let result = do_rename(
            &mock,
            "serial1",
            PathBuf::from("/sdcard/a.txt"),
            PathBuf::from("/sdcard/b.txt"),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mock rename failure"));
    }

    // ── do_delete_all ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn do_delete_all_succeeds() {
        let mock = MockAdbClient::default();
        let paths = vec![
            PathBuf::from("/sdcard/a.txt"),
            PathBuf::from("/sdcard/b.txt"),
        ];
        let result = do_delete_all(&mock, "serial1", paths).await;
        assert!(result.is_ok());

        let calls = mock.calls();
        let delete_calls: Vec<_> = calls.iter().filter(|c| c.contains("delete")).collect();
        assert_eq!(delete_calls.len(), 2, "expected 2 delete calls, got: {calls:?}");
    }

    #[tokio::test]
    async fn do_delete_all_stops_on_first_failure() {
        let mock = MockAdbClient::failing_delete();
        let paths = vec![
            PathBuf::from("/sdcard/a.txt"),
            PathBuf::from("/sdcard/b.txt"),
        ];
        let result = do_delete_all(&mock, "serial1", paths).await;
        assert!(result.is_err());

        let calls = mock.calls();
        let delete_calls: Vec<_> = calls.iter().filter(|c| c.contains("delete")).collect();
        // Should stop after first failure — only 1 delete call
        assert_eq!(delete_calls.len(), 1, "should stop at first failure: {calls:?}");
    }

    #[tokio::test]
    async fn do_delete_all_empty_list_succeeds() {
        let mock = MockAdbClient::default();
        let result = do_delete_all(&mock, "serial1", vec![]).await;
        assert!(result.is_ok());
    }

    // ── resolve_preview_content ───────────────────────────────────────────────

    #[test]
    fn resolve_preview_image_extensions() {
        for ext in ["jpg", "jpeg", "png", "gif"] {
            let path = PathBuf::from(format!("/tmp/test.{ext}"));
            match resolve_preview_content(path) {
                PreviewContent::Image(_) => {}
                other => panic!("expected Image for .{ext}, got {other:?}"),
            }
        }
    }

    #[test]
    fn resolve_preview_unsupported_extension() {
        let path = PathBuf::from("/tmp/binary.exe");
        match resolve_preview_content(path) {
            PreviewContent::Unsupported(msg) => {
                assert!(msg.contains(".exe"), "expected .exe in: {msg}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn resolve_preview_no_extension() {
        let path = PathBuf::from("/tmp/Makefile");
        match resolve_preview_content(path) {
            PreviewContent::Unsupported(_) => {}
            other => panic!("expected Unsupported for no-extension file, got {other:?}"),
        }
    }
}
