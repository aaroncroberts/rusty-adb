//! UI state handlers: view modes, gallery selection, about/settings dialogs,
//! log viewer, error/toast banners, and escape key routing.
use crate::{App, Message};
use iced::Task;
use std::path::PathBuf;
use std::time::Duration;

impl App {
    // ── Pane Layout ───────────────────────────────────────────────────────────

    pub(super) fn expand_pane(&mut self, is_android: bool) -> Task<Message> {
        use super::super::PaneLayout;
        self.pane_layout = if is_android {
            PaneLayout::AndroidExpanded
        } else {
            PaneLayout::LocalExpanded
        };
        Task::none()
    }

    pub(super) fn collapse_panes(&mut self) -> Task<Message> {
        self.pane_layout = super::super::PaneLayout::Split;
        Task::none()
    }

    // ── View Modes ────────────────────────────────────────────────────────────

    pub(super) fn set_local_view_mode(&mut self, mode: super::super::ViewMode) -> Task<Message> {
        self.local_view_mode = mode;
        Task::none()
    }

    pub(super) fn set_android_view_mode(&mut self, mode: super::super::ViewMode) -> Task<Message> {
        self.android_view_mode = mode;
        Task::none()
    }

    pub(super) fn local_gallery_select(&mut self, idx: usize) -> Task<Message> {
        self.local_gallery_idx = idx;
        Task::none()
    }

    pub(super) fn android_gallery_select(&mut self, idx: usize) -> Task<Message> {
        self.android_gallery_idx = idx;
        Task::none()
    }

    // ── About ─────────────────────────────────────────────────────────────────

    pub(super) fn open_about(&mut self) -> Task<Message> {
        self.about_open = true;
        Task::none()
    }

    pub(super) fn close_about(&mut self) -> Task<Message> {
        self.about_open = false;
        Task::none()
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    pub(super) fn open_settings(&mut self) -> Task<Message> {
        self.settings_draft = self.config.clone();
        self.settings_open = true;
        self.preview_modal = None; // close preview if open
        Task::none()
    }

    pub(super) fn close_settings(&mut self) -> Task<Message> {
        self.settings_open = false;
        Task::none()
    }

    pub(super) fn settings_draft_log_level(&mut self, level: String) -> Task<Message> {
        self.settings_draft.log.level = level;
        Task::none()
    }

    pub(super) fn settings_draft_console(&mut self, enabled: bool) -> Task<Message> {
        self.settings_draft.log.console_enabled = enabled;
        Task::none()
    }

    pub(super) fn settings_draft_file(&mut self, enabled: bool) -> Task<Message> {
        self.settings_draft.log.file_enabled = enabled;
        Task::none()
    }

    pub(super) fn save_settings(&mut self) -> Task<Message> {
        self.config = self.settings_draft.clone();
        self.settings_open = false;
        let config = self.config.clone();
        let path = self.config_path.clone();
        tracing::info!(path = %path.display(), "saving settings to config.yml");
        Task::perform(
            async move {
                let yaml = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
                std::fs::write(&path, yaml).map_err(|e| e.to_string())
            },
            |result| match result {
                Ok(()) => Message::ShowToast(
                    "Settings saved — changes apply on next launch".to_string(),
                ),
                Err(e) => Message::ShowError(format!("Failed to save settings: {e}")),
            },
        )
    }

    pub(super) fn open_log_folder(&mut self) -> Task<Message> {
        if let Some(dir) = self.config_path.parent() {
            let dir = dir.to_string_lossy().into_owned();
            tracing::info!(dir = %dir, "opening log folder");
            #[cfg(target_os = "macos")]
            if let Err(e) = std::process::Command::new("open").arg(&dir).spawn() {
                tracing::warn!(error = %e, dir = %dir, "failed to open log folder in Finder");
            }
            #[cfg(target_os = "windows")]
            if let Err(e) = std::process::Command::new("explorer").arg(&dir).spawn() {
                tracing::warn!(error = %e, dir = %dir, "failed to open log folder in Explorer");
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            if let Err(e) = std::process::Command::new("xdg-open").arg(&dir).spawn() {
                tracing::warn!(error = %e, dir = %dir, "failed to open log folder with xdg-open");
            }
        }
        Task::none()
    }

    // ── Log Viewer ────────────────────────────────────────────────────────────

    pub(super) fn open_log_viewer(&mut self) -> Task<Message> {
        self.log_viewer_open = true;
        // Scan the app dir for *.log files and load the most-recent one
        let log_dir = self
            .config_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        Task::perform(
            async move {
                let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&log_dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().and_then(|e| e.to_str()) == Some("log")
                    })
                    .collect();
                // Sort newest-first by filename (date is embedded: rusty-adb.YYYY-MM-DD.log)
                files.sort_by(|a, b| b.cmp(a));
                files
            },
            Message::LogViewerFilesLoaded,
        )
    }

    pub(super) fn close_log_viewer(&mut self) -> Task<Message> {
        self.log_viewer_open = false;
        Task::none()
    }

    pub(super) fn log_viewer_files_loaded(&mut self, files: Vec<PathBuf>) -> Task<Message> {
        self.log_viewer_files = files;
        // Auto-load the first (newest) file
        if let Some(first) = self.log_viewer_files.first().cloned() {
            self.update(Message::LogViewerSelectFile(first))
        } else {
            self.log_viewer_content = "(No log files found)".to_string();
            Task::none()
        }
    }

    pub(super) fn log_viewer_select_file(&mut self, path: PathBuf) -> Task<Message> {
        self.log_viewer_selected = Some(path.clone());
        Task::perform(
            async move {
                std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| format!("(Failed to read log: {e})"))
            },
            Message::LogViewerFileLoaded,
        )
    }

    pub(super) fn log_viewer_file_loaded(&mut self, content: String) -> Task<Message> {
        self.log_viewer_content = content;
        Task::none()
    }

    pub(super) fn log_viewer_set_level(&mut self, level: super::super::LogLevel) -> Task<Message> {
        self.log_viewer_level = level;
        Task::none()
    }

    // ── Error / Toast ─────────────────────────────────────────────────────────

    pub(super) fn show_error(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "showing error banner");
        self.error_banner = Some(msg);
        // Schedule auto-dismiss after 3 seconds
        Task::perform(
            async { tokio::time::sleep(Duration::from_secs(3)).await },
            |_| Message::DismissError,
        )
    }

    pub(super) fn dismiss_error(&mut self) -> Task<Message> {
        self.error_banner = None;
        Task::none()
    }

    pub(super) fn show_toast(&mut self, msg: String) -> Task<Message> {
        tracing::info!(toast = %msg, "showing info toast");
        self.toast = Some(msg);
        Task::perform(
            async { tokio::time::sleep(Duration::from_secs(3)).await },
            |_| Message::DismissToast,
        )
    }

    pub(super) fn dismiss_toast(&mut self) -> Task<Message> {
        self.toast = None;
        Task::none()
    }

    // ── Escape routing ────────────────────────────────────────────────────────

    pub(super) fn escape_pressed(&mut self) -> Task<Message> {
        if self.log_viewer_open {
            return self.update(Message::CloseLogViewer);
        }
        if self.settings_open {
            return self.update(Message::CloseSettings);
        }
        if self.about_open {
            return self.update(Message::CloseAbout);
        }
        if self.preview_modal.is_some() {
            return self.update(Message::ClosePreview);
        }
        self.update(Message::AndroidRenameCancel)
    }
}
