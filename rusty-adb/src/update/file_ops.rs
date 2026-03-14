//! File operation handlers: rename, delete, and preview.
use crate::{App, Message, PreviewContent};
use crate::file_pane;
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
            async move {
                client
                    .rename(&serial, &from_path, &to_path)
                    .await
                    .map_err(|e| e.to_string())
            },
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
            async move {
                for path in &paths {
                    client
                        .delete(&serial, path)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok::<(), String>(())
            },
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

    pub(super) fn preview_file(&mut self, entry: crate::filesystem::DirEntry) -> Task<Message> {
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
        let ext = local_path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let content = match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" => PreviewContent::Image(local_path),
            "txt" | "log" | "json" | "xml" | "md" | "toml" | "yaml" | "yml" => {
                match std::fs::read_to_string(&local_path) {
                    Ok(text) => PreviewContent::Text(text),
                    Err(e) => {
                        PreviewContent::Unsupported(format!("Could not read file: {e}"))
                    }
                }
            }
            other => PreviewContent::Unsupported(format!(
                "Preview not available for .{other} files"
            )),
        };
        self.preview_modal = Some(content);
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
