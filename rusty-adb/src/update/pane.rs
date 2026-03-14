//! Pane navigation handlers: local pane and android pane navigation,
//! sorting, selection, RefreshPanes, DisconnectDevice, SpinnerTick.
use crate::{App, Message};
use crate::fs::{android_entry_to_dir_entry, DirEntry, FileSystem, LocalFs, SortField};
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
                Ok(entries) => Message::LocalEntriesLoaded { path: load_path.clone(), entries },
                Err(e) => Message::LocalLoadError(e),
            },
        )
    }

    pub(super) fn local_entries_loaded(&mut self, path: PathBuf, entries: Vec<DirEntry>) -> Task<Message> {
        self.local_pane.on_entries_loaded(path, entries);
        Task::none()
    }

    pub(super) fn local_load_error(&mut self, msg: String) -> Task<Message> {
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
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            return Task::none();
        };

        self.android_pane.begin_navigate(path.clone());

        Task::perform(
            async move {
                // Fetch directory entries and storage roots in parallel
                let entries_fut = client.list_dir(&serial, &path);
                let roots_fut = client.list_storage_roots(&serial);
                let (entries_res, roots) = tokio::join!(entries_fut, roots_fut);
                entries_res
                    .map(|raw| {
                        let entries: Vec<DirEntry> = raw
                            .into_iter()
                            .map(android_entry_to_dir_entry)
                            .collect();
                        (path, entries, roots)
                    })
                    .map_err(|e| e.to_string())
            },
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
        // Update storage roots in the context so is_nav_root works correctly
        if let Some(ref mut ctx) = self.android_ctx {
            ctx.storage_roots = roots.clone();
        }
        self.android_pane.storage_roots = roots;
        self.android_pane.on_entries_loaded(path, entries);
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
            |result| match result {
                Ok(_) | Err(_) => Message::PollDevices,
            },
        )
    }
}
