//! Transfer handlers: CopyToAndroid, CopyToLocal, TransferProgress,
//! TransferComplete, TransferCancelled, TransferFailed, CancelTransfer,
//! and the drag-and-drop FileHovered/FilesHoveredLeft/FileDropped handlers.
use crate::{App, Message};
use crate::status_bar::TransferStatus;
use crate::transfer::{TransferDirection, TransferJob};
use iced::Task;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::VecDeque;

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
        let direction = self.active_transfer.as_ref().map(|j| j.direction.clone());

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

        // Refresh the destination pane so the new file is visible
        match direction {
            Some(TransferDirection::ToAndroid) => {
                let path = self.android_pane.current_path.clone();
                return self.update(Message::AndroidNavigateTo(path));
            }
            Some(TransferDirection::ToLocal) => {
                let path = self.local_pane.current_path.clone();
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

    pub(super) fn file_dropped(&mut self, path: std::path::PathBuf) -> Task<Message> {
        self.file_hover_active = false;

        // Guard: need a connected device and adb client
        let (Some(client), Some(serial)) = (&self.adb_client, &self.active_serial) else {
            tracing::warn!(path = %path.display(), "file dropped but no device connected");
            return self.update(Message::ShowError(
                "No device connected — connect a device before dropping files.".to_string(),
            ));
        };

        // Skip directories (first iteration: files only)
        if path.is_dir() {
            tracing::warn!(path = %path.display(), "directory drop ignored (not supported yet)");
            return Task::none();
        }

        let android_dir = self.android_pane.current_path.clone();
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        self.transfer_id += 1;
        let job = TransferJob {
            id: self.transfer_id,
            adb_path: client.adb_path.clone(),
            serial: serial.clone(),
            source: path.clone(),
            destination: android_dir,
            direction: TransferDirection::ToAndroid,
            filename: filename.clone(),
        };

        tracing::info!(file = %path.display(), "file dropped → queuing transfer to android");

        if self.active_transfer.is_none() {
            // No transfer running — start this job immediately
            let cancel = Arc::new(AtomicBool::new(false));
            self.cancel_flag = Some(cancel);
            self.transfer_queue_done = 0;
            self.transfer_queue_total = 1;
            self.transfer_status = Some(TransferStatus {
                filename,
                percent: 0,
                speed_display: String::new(),
                job_index: 1,
                job_total: 1,
            });
            self.active_transfer = Some(job);
        } else {
            // A transfer is already running — add to queue
            self.transfer_queue.push_back(job);
            self.transfer_queue_total += 1;
        }

        Task::none()
    }
}
