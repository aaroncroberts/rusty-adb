//! Queue message handlers: copy confirm dialog, queue management, copy engine dispatch.

use crate::adb::{TransferDirection, TransferJob};
use crate::queue::QueueStatus;
use crate::{App, Message};
use iced::Task;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

impl App {
    // ── Copy Confirm Dialog ───────────────────────────────────────────────────

    pub(super) fn show_copy_confirm(&mut self) -> Task<Message> {
        tracing::debug!("copy confirm dialog opened");
        self.copy_confirm_open = true;
        Task::none()
    }

    pub(super) fn close_copy_confirm(&mut self) -> Task<Message> {
        self.copy_confirm_open = false;
        Task::none()
    }

    pub(super) fn confirm_copy_to_device(&mut self) -> Task<Message> {
        self.copy_confirm_open = false;
        let dest = self.android_pane.current_path.clone();
        let selected_paths: Vec<PathBuf> = self
            .local_pane
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| self.local_pane.selected.contains(i))
            .map(|(_, e)| e.path.clone())
            .collect();

        let count = selected_paths.len();
        if count == 0 {
            tracing::warn!("confirm_copy_to_device: no paths resolved — aborting");
            return Task::none();
        }

        for path in selected_paths {
            let item_dest = dest.join(path.file_name().unwrap_or_default());
            let id = self.copy_queue.enqueue(path, item_dest);
            tracing::info!(id, "item enqueued for copy");
        }

        tracing::info!(count, dest = %dest.display(), "items added to copy queue");

        // Kick off the copy engine for the first pending item
        self.try_start_next_queue_copy()
            .chain(self.update(Message::ShowToast(format!(
                "{count} item{} added to copy queue",
                if count == 1 { "" } else { "s" }
            ))))
    }

    // ── Queue Management Dialog ───────────────────────────────────────────────

    pub(super) fn open_queue_dialog(&mut self) -> Task<Message> {
        tracing::debug!("queue dialog opened");
        self.queue_open = true;
        Task::none()
    }

    pub(super) fn close_queue_dialog(&mut self) -> Task<Message> {
        self.queue_open = false;
        self.queue_editing = None;
        Task::none()
    }

    pub(super) fn toggle_queue_pause(&mut self) -> Task<Message> {
        let new_state = !self.copy_queue.is_paused;
        self.copy_queue.set_paused(new_state);
        if !new_state {
            // Resumed — start copying if there's a pending item
            return self.try_start_next_queue_copy();
        }
        Task::none()
    }

    pub(super) fn queue_remove_item(&mut self, id: u64) -> Task<Message> {
        self.copy_queue.remove(id);
        Task::none()
    }

    pub(super) fn queue_edit_item(&mut self, id: u64) -> Task<Message> {
        let current_dest = self
            .copy_queue
            .items
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.android_dest.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.queue_editing = Some((id, current_dest));
        Task::none()
    }

    pub(super) fn queue_edit_dest_input(&mut self, value: String) -> Task<Message> {
        if let Some((_, ref mut input)) = self.queue_editing {
            *input = value;
        }
        Task::none()
    }

    pub(super) fn queue_edit_dest_confirm(&mut self) -> Task<Message> {
        if let Some((id, ref dest)) = self.queue_editing.clone() {
            let new_dest = PathBuf::from(dest.trim());
            if !new_dest.as_os_str().is_empty() {
                self.copy_queue.update_dest(id, new_dest);
                tracing::info!(id, "queue item destination updated");
            }
            self.queue_editing = None;
        }
        Task::none()
    }

    // ── Copy Engine ───────────────────────────────────────────────────────────

    /// Start copying the next pending queue item if one exists and no other
    /// item is already actively copying.  Returns a `Task::none()` if there's
    /// nothing to do.
    pub(super) fn try_start_next_queue_copy(&mut self) -> Task<Message> {
        // Don't start if already copying something
        let already_copying = self
            .copy_queue
            .items
            .iter()
            .any(|i| matches!(i.status, QueueStatus::Copying { .. }));
        if already_copying {
            return Task::none();
        }

        // Find the next pending item
        let Some(item) = self.copy_queue.next_pending() else {
            return Task::none();
        };
        let Some(client) = self.adb_client.clone() else {
            tracing::warn!("try_start_next_queue_copy: no adb_client — cannot copy");
            return Task::none();
        };
        let Some(serial) = self.active_serial.clone() else {
            tracing::warn!("try_start_next_queue_copy: no active_serial — cannot copy");
            return Task::none();
        };

        let id = item.id;
        let local_path = item.local_path.clone();
        let android_dest = item.android_dest.clone();
        let filename = local_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| local_path.display().to_string());

        tracing::info!(
            id,
            local = %local_path.display(),
            dest  = %android_dest.display(),
            "queue: starting copy"
        );

        self.copy_queue
            .update_status(id, QueueStatus::Copying { percent: 0 });

        let job = TransferJob {
            id,
            adb_path: client.adb_path.clone(),
            serial,
            source: local_path,
            destination: android_dest,
            direction: TransferDirection::ToAndroid,
            filename,
        };
        let cancel = Arc::new(AtomicBool::new(false));

        Task::perform(
            async move {
                // Capture adb-level failures (exit 1, "Permission denied", etc.)
                // run_transfer always returns Ok(()); failures come via the callback.
                let failure: std::sync::Arc<std::sync::Mutex<Option<String>>> =
                    std::sync::Arc::new(std::sync::Mutex::new(None));
                let failure_capture = std::sync::Arc::clone(&failure);

                crate::adb::run_transfer(&job, cancel, move |event| {
                    if let crate::adb::TransferEvent::Failed(reason) = &event {
                        if let Ok(mut g) = failure_capture.lock() {
                            *g = Some(reason.clone());
                        }
                    }
                })
                .await
                .map_err(|e| (id, e.to_string()))?;

                // If the callback recorded a failure, surface it as Err
                if let Some(reason) = failure.lock().ok().and_then(|mut g| g.take()) {
                    tracing::warn!(id, error = %reason, "queue: adb push failed");
                    return Err((id, reason));
                }
                Ok(id)
            },
            |result| match result {
                Ok(id) => Message::QueueItemComplete(id),
                Err((id, reason)) => Message::QueueItemFailed { id, reason },
            },
        )
    }

    pub(super) fn queue_item_complete(&mut self, id: u64) -> Task<Message> {
        tracing::info!(id, "queue: item copy complete");
        self.copy_queue.update_status(id, QueueStatus::Done);

        // Refresh the destination directory so the newly copied file appears.
        // The item's android_dest is the full file path; parent() gives the dir.
        let dest_dir = self
            .copy_queue
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.android_dest.parent())
            .map(|dir| dir.to_path_buf());

        let next = self.try_start_next_queue_copy();
        if let Some(dir) = dest_dir {
            tracing::debug!(path = %dir.display(), "refreshing android pane after queue copy");
            next.chain(Task::done(Message::AndroidNavigateTo(dir)))
        } else {
            next
        }
    }

    pub(super) fn queue_item_failed(&mut self, id: u64, reason: String) -> Task<Message> {
        tracing::warn!(id, error = %reason, "queue: item copy failed");
        // Get the filename for a useful error message before updating status
        let filename = self
            .copy_queue
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.local_path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("item {id}"));
        self.copy_queue
            .update_status(id, QueueStatus::Failed { reason: reason.clone() });
        // Show a visible error banner so the user knows the copy failed
        let next = self.try_start_next_queue_copy();
        next.chain(self.update(Message::ShowError(format!(
            "Copy failed for \"{filename}\": {reason}"
        ))))
    }

}
