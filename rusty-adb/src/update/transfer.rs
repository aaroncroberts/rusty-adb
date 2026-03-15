//! Transfer handlers: CopyToAndroid, CopyToLocal, TransferProgress,
//! TransferComplete, TransferCancelled, TransferFailed, CancelTransfer,
//! and the drag-and-drop FileHovered/FilesHoveredLeft/FileDropped handlers.
use crate::adb::{TransferDirection, TransferJob, TransferStatus};
use crate::{App, Message};
use iced::Task;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

impl App {
    pub(super) fn copy_to_android(&mut self) -> Task<Message> {
        let Some(client) = &self.adb_client else {
            return Task::none();
        };
        let Some(serial) = &self.active_serial else {
            return Task::none();
        };
        let android_dir = self.android_pane.current_path.clone();

        // Build a job for each selected non-directory local file
        let mut jobs: VecDeque<TransferJob> = self
            .local_pane
            .selected
            .iter()
            .filter_map(|&i| self.local_pane.entries.get(i))
            .filter(|e| !e.is_dir)
            .map(|e| {
                self.transfer_id += 1;
                TransferJob {
                    id: self.transfer_id,
                    adb_path: client.adb_path.clone(),
                    serial: serial.clone(),
                    source: e.path.clone(),
                    destination: android_dir.clone(),
                    direction: TransferDirection::ToAndroid,
                    filename: e.name.clone(),
                }
            })
            .collect();

        if jobs.is_empty() {
            tracing::warn!("CopyToAndroid: no files selected");
            return Task::none();
        }

        let total = jobs.len();
        // Safety: guarded by is_empty() check above
        let first = jobs.pop_front().expect("queue non-empty: guarded above");
        tracing::info!(total, file = %first.source.display(), "starting copy → android");

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel);
        self.transfer_queue = jobs;
        self.transfer_queue_done = 0;
        self.transfer_queue_total = total;

        let filename = first.filename.clone();
        self.active_transfer = Some(first);
        self.transfer_status = Some(TransferStatus {
            filename,
            percent: 0,
            speed_display: String::new(),
            job_index: 1,
            job_total: total,
        });

        Task::none()
    }

    pub(super) fn copy_to_local(&mut self) -> Task<Message> {
        let Some(client) = &self.adb_client else {
            return Task::none();
        };
        let Some(serial) = &self.active_serial else {
            return Task::none();
        };
        let local_dir = self.local_pane.current_path.clone();

        // Build a job for each selected non-directory android file
        let mut jobs: VecDeque<TransferJob> = self
            .android_pane
            .selected
            .iter()
            .filter_map(|&i| self.android_pane.entries.get(i))
            .filter(|e| !e.is_dir)
            .map(|e| {
                self.transfer_id += 1;
                TransferJob {
                    id: self.transfer_id,
                    adb_path: client.adb_path.clone(),
                    serial: serial.clone(),
                    source: e.path.clone(),
                    destination: local_dir.clone(),
                    direction: TransferDirection::ToLocal,
                    filename: e.name.clone(),
                }
            })
            .collect();

        if jobs.is_empty() {
            tracing::warn!("CopyToLocal: no files selected");
            return Task::none();
        }

        let total = jobs.len();
        // Safety: guarded by is_empty() check above
        let first = jobs.pop_front().expect("queue non-empty: guarded above");
        tracing::info!(total, file = %first.source.display(), "starting copy ← android");

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel);
        self.transfer_queue = jobs;
        self.transfer_queue_done = 0;
        self.transfer_queue_total = total;

        let filename = first.filename.clone();
        self.active_transfer = Some(first);
        self.transfer_status = Some(TransferStatus {
            filename,
            percent: 0,
            speed_display: String::new(),
            job_index: 1,
            job_total: total,
        });

        Task::none()
    }

    pub(super) fn transfer_progress(&mut self, percent: u8) -> Task<Message> {
        if let Some(status) = &mut self.transfer_status {
            status.percent = percent;
        }
        Task::none()
    }

    pub(super) fn transfer_complete(&mut self, speed_display: String) -> Task<Message> {
        tracing::info!(speed = %speed_display, "transfer complete");
        // Capture direction and destination BEFORE clearing active_transfer below
        let direction = self.active_transfer.as_ref().map(|j| j.direction.clone());
        let completed_dest = self.active_transfer.as_ref().map(|j| j.destination.clone());

        self.transfer_queue_done += 1;

        // Pop the next queued job, if any
        if let Some(next) = self.transfer_queue.pop_front() {
            let total = self.transfer_queue_total;
            let done = self.transfer_queue_done;
            let filename = next.filename.clone();

            // Fresh cancel flag for the next job
            let cancel = Arc::new(AtomicBool::new(false));
            self.cancel_flag = Some(cancel);
            self.active_transfer = Some(next);
            self.transfer_status = Some(TransferStatus {
                filename,
                percent: 0,
                speed_display: String::new(),
                job_index: done + 1,
                job_total: total,
            });
            return Task::none();
        }

        // All done
        self.active_transfer = None;
        self.transfer_status = None;
        self.cancel_flag = None;

        // Refresh the destination pane so the new file is visible.
        // Use the job's destination path (captured before clear above), not the
        // pane's current_path — the user may have navigated elsewhere mid-transfer.
        match direction {
            Some(TransferDirection::ToAndroid) => {
                let path = completed_dest
                    .unwrap_or_else(|| self.android_pane.current_path.clone());
                tracing::debug!(path = %path.display(), "refreshing android pane after transfer");
                return self.update(Message::AndroidNavigateTo(path));
            }
            Some(TransferDirection::ToLocal) => {
                let path = completed_dest
                    .unwrap_or_else(|| self.local_pane.current_path.clone());
                tracing::debug!(path = %path.display(), "refreshing local pane after transfer");
                return self.update(Message::LocalNavigateTo(path));
            }
            None => {}
        }

        Task::none()
    }

    pub(super) fn transfer_failed(&mut self, msg: String) -> Task<Message> {
        tracing::warn!(error = %msg, "transfer failed");
        self.active_transfer = None;
        self.transfer_status = None;
        self.transfer_queue.clear();
        self.cancel_flag = None;
        self.update(Message::ShowError(format!("Transfer failed: {msg}")))
    }

    pub(super) fn cancel_transfer(&mut self) -> Task<Message> {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
            tracing::info!("cancel requested");
        }
        // Clear queue so no more jobs start after this one stops
        self.transfer_queue.clear();
        Task::none()
    }

    pub(super) fn transfer_cancelled(&mut self) -> Task<Message> {
        tracing::info!("transfer cancelled");
        self.active_transfer = None;
        self.transfer_status = None;
        self.transfer_queue.clear();
        self.cancel_flag = None;
        Task::none()
    }

    pub(super) fn file_hovered(&mut self) -> Task<Message> {
        self.file_hover_active = true;
        Task::none()
    }

    pub(super) fn files_hovered_left(&mut self) -> Task<Message> {
        self.file_hover_active = false;
        Task::none()
    }

    // ── In-app drag-and-drop ──────────────────────────────────────────────────

    pub(super) fn local_drag_started(&mut self) -> Task<Message> {
        // Only start a drag if there are local files selected (not dirs-only)
        let has_files = self.local_pane.selected.iter().any(|&i| {
            self.local_pane.entries.get(i).map(|e| !e.is_dir).unwrap_or(false)
        });
        if has_files && self.active_serial.is_some() {
            self.drag_in_progress = true;
        }
        Task::none()
    }

    pub(super) fn dropped_on_android(&mut self) -> Task<Message> {
        if !self.drag_in_progress {
            return Task::none();
        }
        self.drag_in_progress = false;

        if self.active_serial.is_none() {
            return Task::none();
        }

        let dest = self.android_pane.current_path.clone();
        let selected_paths: Vec<_> = self
            .local_pane
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| self.local_pane.selected.contains(i) && !e.is_dir)
            .map(|(_, e)| e.path.clone())
            .collect();

        let count = selected_paths.len();
        if count == 0 {
            return Task::none();
        }

        for path in selected_paths {
            let item_dest = dest.join(path.file_name().unwrap_or_default());
            let id = self.copy_queue.enqueue(path, item_dest);
            tracing::info!(id, "in-app drag drop → enqueued");
        }

        tracing::info!(count, dest = %dest.display(), "drag-and-drop: items enqueued");

        self.try_start_next_queue_copy()
            .chain(self.update(Message::ShowToast(format!(
                "{count} item{} added to copy queue",
                if count == 1 { "" } else { "s" }
            ))))
    }

    pub(super) fn drag_cancelled(&mut self) -> Task<Message> {
        self.drag_in_progress = false;
        Task::none()
    }

    pub(super) fn file_dropped(&mut self, path: std::path::PathBuf) -> Task<Message> {
        self.file_hover_active = false;

        // Guard: need a connected device
        if self.active_serial.is_none() {
            tracing::warn!(path = %path.display(), "file dropped but no device connected");
            return self.update(Message::ShowError(
                "No device connected — connect a device before dropping files.".to_string(),
            ));
        }

        // Skip directories (files only)
        if path.is_dir() {
            tracing::warn!(path = %path.display(), "directory drop ignored");
            return Task::none();
        }

        let android_dir = self.android_pane.current_path.clone();
        let dest = android_dir.join(path.file_name().unwrap_or_default());

        let id = self.copy_queue.enqueue(path.clone(), dest);
        tracing::info!(file = %path.display(), id, "OS file drop → enqueued in copy queue");

        self.try_start_next_queue_copy()
            .chain(self.update(Message::ShowToast(
                "1 item added to copy queue".to_string(),
            )))
    }
}
