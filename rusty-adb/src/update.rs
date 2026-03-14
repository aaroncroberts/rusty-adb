//! Message update handlers for `App`.
use super::{App, Message, PreviewContent};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use iced::widget::text_input;
use iced::Task;
use crate::adb::{AdbClient, AdbDevice, DeviceState};
use crate::android_fs::{android_entry_to_dir_entry, AndroidContext};
use crate::file_pane;
use crate::filesystem::{DirEntry, FileSystem, PaneState};
use crate::local_fs::LocalFs;
use crate::status_bar::{AdbStatus, TransferStatus};
use crate::transfer::{TransferDirection, TransferJob};

impl App {
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── View Modes ────────────────────────────────────────────────────
            Message::SetLocalViewMode(mode) => {
                self.local_view_mode = mode;
                Task::none()
            }
            Message::SetAndroidViewMode(mode) => {
                self.android_view_mode = mode;
                Task::none()
            }
            Message::LocalGallerySelect(idx) => {
                self.local_gallery_idx = idx;
                Task::none()
            }
            Message::AndroidGallerySelect(idx) => {
                self.android_gallery_idx = idx;
                Task::none()
            }

            // ── ADB ──────────────────────────────────────────────────────────
            Message::AdbReady(client) => {
                self.adb_client = Some((*client).clone());
                self.update(Message::PollDevices)
            }

            Message::PollDevices => {
                let Some(client) = self.adb_client.clone() else {
                    return Task::none();
                };
                Task::perform(
                    async move {
                        client
                            .list_devices()
                            .await
                            .map(Arc::new)
                            .map_err(|e| e.to_string())
                    },
                    |result| match result {
                        Ok(d) => Message::DevicesLoaded(d),
                        Err(e) => Message::AdbError(e),
                    },
                )
            }

            Message::DevicesLoaded(devices) => {
                // Successful poll resets the watchdog counter
                self.daemon_error_count = 0;
                self.devices = (*devices).clone();
                self.adb_status = derive_status(&self.devices);
                tracing::debug!(count = self.devices.len(), "devices refreshed");

                // Detect newly-connected authorized device
                let new_serial = self
                    .devices
                    .iter()
                    .find(|d| d.state == DeviceState::Device)
                    .map(|d| d.serial.clone());

                match (&self.active_serial, &new_serial) {
                    (None, Some(serial)) => {
                        // Device just appeared — build context and start loading /sdcard
                        tracing::info!(serial = %serial, "device connected, loading /sdcard");
                        let device_label = self.devices.iter()
                            .find(|d| &d.serial == serial)
                            .and_then(|d| d.model.clone())
                            .unwrap_or_else(|| serial.clone());
                        let client = self.adb_client.clone()
                            .expect("adb_client set at AdbReady");
                        self.active_serial = Some(serial.clone());
                        self.android_ctx = Some(AndroidContext {
                            client,
                            serial: serial.clone(),
                            storage_roots: Vec::new(),
                            device_label,
                        });
                        self.android_pane.state = PaneState::Loading;
                        return self.update(Message::AndroidNavigateTo(PathBuf::from("/sdcard")));
                    }
                    (Some(_), None) => {
                        // Device disconnected — clear context and reset pane
                        tracing::info!("device disconnected");
                        self.active_serial = None;
                        self.active_transfer = None;
                        self.transfer_status = None;
                        self.android_ctx = None;
                        self.android_pane.state = PaneState::NoDevice;
                        self.android_pane.entries.clear();
                        self.android_pane.selected.clear();
                    }
                    _ => {}
                }

                Task::none()
            }

            Message::AdbError(msg) => {
                self.daemon_error_count += 1;
                tracing::warn!(
                    error = %msg,
                    count = self.daemon_error_count,
                    "adb poll error"
                );
                if self.daemon_error_count == 3 {
                    // Three consecutive failures — attempt auto-restart before surfacing to user
                    tracing::warn!("3 consecutive poll errors — attempting daemon auto-restart");
                    self.daemon_error_count = 0;
                    return self.update(Message::RestartDaemon);
                }
                if self.daemon_error_count >= 4 {
                    // Restart didn't help — surface a persistent, actionable error
                    let msg = "ADB daemon unresponsive".to_string();
                    self.adb_status = AdbStatus::Error(msg.clone());
                    return self.update(Message::ShowError(
                        "ADB daemon unresponsive. Use 'Restart Daemon' in the toolbar.".to_string(),
                    ));
                }
                // Counts 1–2: transient failures — suppress to avoid spurious error toasts
                Task::none()
            }

            // ── Local Pane ────────────────────────────────────────────────────
            Message::LocalNavigateTo(path) => {
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
            Message::LocalEntriesLoaded { path, entries } => {
                self.local_pane.on_entries_loaded(path, entries);
                Task::none()
            }
            Message::LocalLoadError(msg) => {
                self.local_pane.on_error(msg);
                Task::none()
            }
            Message::LocalSelectEntry(i) => {
                self.local_pane.select(i);
                Task::none()
            }
            Message::LocalToggleHidden => {
                self.local_pane.toggle_hidden();
                // Reload after toggling hidden (shows/hides the dot-files)
                let path = self.local_pane.current_path.clone();
                self.update(Message::LocalNavigateTo(path))
            }
            Message::LocalToggleType => {
                self.local_pane.toggle_type();
                Task::none()
            }
            Message::LocalToggleSize => {
                self.local_pane.toggle_size();
                Task::none()
            }
            Message::LocalToggleModified => {
                self.local_pane.toggle_modified();
                Task::none()
            }
            Message::LocalSortBy(field) => {
                self.local_pane.set_sort(field);
                Task::none()
            }
            Message::AndroidSortBy(field) => {
                self.android_pane.set_sort(field);
                Task::none()
            }

            // ── Android Pane ──────────────────────────────────────────────────
            Message::AndroidToggleHidden => {
                self.android_pane.toggle_hidden();
                Task::none()
            }
            Message::AndroidToggleType => {
                self.android_pane.toggle_type();
                Task::none()
            }
            Message::AndroidToggleSize => {
                self.android_pane.toggle_size();
                Task::none()
            }
            Message::AndroidToggleModified => {
                self.android_pane.toggle_modified();
                Task::none()
            }
            Message::AndroidNavigateTo(path) => {
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

            Message::AndroidEntriesLoaded { path, entries, roots } => {
                // Update storage roots in the context so is_nav_root works correctly
                if let Some(ref mut ctx) = self.android_ctx {
                    ctx.storage_roots = roots.clone();
                }
                self.android_pane.storage_roots = roots;
                self.android_pane.on_entries_loaded(path, entries);
                Task::none()
            }

            Message::AndroidLoadError(msg) => {
                tracing::warn!(error = %msg, "android directory load error");
                self.android_pane.on_error(msg);
                Task::none()
            }

            Message::AndroidSelectEntry(i) => {
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

            Message::SpinnerTick => {
                self.android_pane.tick_spinner();
                Task::none()
            }

            // ── Navigation shortcuts ───────────────────────────────────────────
            Message::RefreshPanes => {
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

            Message::LocalNavigateUp => {
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

            Message::DisconnectDevice => {
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

            // ── Transfer ──────────────────────────────────────────────────────
            Message::CopyToAndroid => {
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

            Message::CopyToLocal => {
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

            Message::TransferProgress { percent } => {
                if let Some(status) = &mut self.transfer_status {
                    status.percent = percent;
                }
                Task::none()
            }

            Message::TransferComplete { speed_display } => {
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

            Message::TransferFailed(msg) => {
                tracing::warn!(error = %msg, "transfer failed");
                self.active_transfer = None;
                self.transfer_status = None;
                self.transfer_queue.clear();
                self.cancel_flag = None;
                self.update(Message::ShowError(format!("Transfer failed: {msg}")))
            }

            Message::CancelTransfer => {
                if let Some(flag) = &self.cancel_flag {
                    flag.store(true, Ordering::Relaxed);
                    tracing::info!("cancel requested");
                }
                // Clear queue so no more jobs start after this one stops
                self.transfer_queue.clear();
                Task::none()
            }

            Message::TransferCancelled => {
                tracing::info!("transfer cancelled");
                self.active_transfer = None;
                self.transfer_status = None;
                self.transfer_queue.clear();
                self.cancel_flag = None;
                Task::none()
            }

            Message::ShowError(msg) => {
                tracing::warn!(error = %msg, "showing error banner");
                self.error_banner = Some(msg);
                // Schedule auto-dismiss after 3 seconds
                Task::perform(
                    async { tokio::time::sleep(Duration::from_secs(3)).await },
                    |_| Message::DismissError,
                )
            }

            Message::DismissError => {
                self.error_banner = None;
                Task::none()
            }

            // ── ADB install flow ──────────────────────────────────────────────
            Message::AdbNotFound => {
                self.adb_status = AdbStatus::NotFound;
                Task::none()
            }

            Message::RetryAdbFind => {
                tracing::info!("retrying adb detection after install");
                self.install_log.clear();
                self.installing = false;
                Task::perform(
                    async {
                        let client = AdbClient::find().await.map_err(|e| e.to_string())?;
                        if let Err(e) = client.start_server().await {
                            return Err(format!("daemon:{}", e));
                        }
                        Ok::<AdbClient, String>(client)
                    },
                    |result| match result {
                        Ok(client) => {
                            tracing::info!(adb = %client.adb_path.display(), "adb found on retry");
                            Message::AdbReady(Arc::new(client))
                        }
                        Err(e) if e.starts_with("adb not found") => Message::AdbNotFound,
                        Err(e) if e.starts_with("daemon:") => {
                            Message::DaemonStartFailed(e["daemon:".len()..].to_string())
                        }
                        Err(e) => Message::AdbError(e),
                    },
                )
            }

            Message::InstallAdb => {
                if self.installing {
                    return Task::none(); // guard against double-tap
                }
                self.installing = true;
                self.install_log.clear();
                self.install_log.push("Starting installation…".to_string());
                tracing::info!("starting platform-tools install via package manager");

                Task::perform(
                    async {
                        #[cfg(target_os = "macos")]
                        let mut child = tokio::process::Command::new("brew")
                            .args(["install", "--cask", "android-platform-tools"])
                            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                            .map_err(|e| {
                                format!(
                                    "brew not found: {e}. Install Homebrew from https://brew.sh"
                                )
                            })?;

                        #[cfg(target_os = "windows")]
                        let mut child = tokio::process::Command::new("winget")
                            .args(["install", "--id", "Google.PlatformTools", "--accept-source-agreements", "--accept-package-agreements"])
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                            .map_err(|e| format!("winget not found: {e}. Install App Installer from the Microsoft Store"))?;

                        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                        return Err("Automatic install not supported on this platform. Download from developer.android.com/tools/releases/platform-tools".to_string());

                        let status = child.wait().await.map_err(|e| e.to_string())?;
                        if status.success() {
                            Ok(())
                        } else {
                            let code = status.code().unwrap_or(-1);
                            Err(format!("Package manager exited with code {code}. Check the log above for details."))
                        }
                    },
                    |result| match result {
                        Ok(()) => Message::InstallComplete,
                        Err(e) => Message::InstallFailed(e),
                    },
                )
            }

            Message::InstallComplete => {
                tracing::info!("platform-tools install succeeded, retrying adb detection");
                self.install_log
                    .push("[OK] Installation complete. Detecting adb...".to_string());
                self.installing = false;
                self.update(Message::RetryAdbFind)
            }

            Message::InstallFailed(err) => {
                tracing::warn!(error = %err, "platform-tools install failed");
                self.installing = false;
                self.install_log.push(format!("[ERR] {err}"));
                Task::none()
            }

            Message::DaemonStartFailed(msg) => {
                tracing::warn!(error = %msg, "adb start-server failed");
                self.adb_status = AdbStatus::Error(format!("Daemon error: {msg}"));
                // Show error banner with note that user can restart daemon
                self.update(Message::ShowError(format!(
                    "ADB daemon failed to start: {msg}. Use 'Restart Daemon' in the toolbar."
                )))
            }

            Message::RestartDaemon => {
                tracing::info!("restarting adb daemon (kill-server + start-server)");
                let client = match &self.adb_client {
                    Some(c) => c.clone(),
                    None => return Task::none(),
                };
                Task::perform(
                    async move {
                        // kill-server first (ignore errors — may already be dead)
                        let _ = tokio::process::Command::new(&client.adb_path)
                            .arg("kill-server")
                            .status()
                            .await;
                        // start-server fresh
                        client.start_server().await.map_err(|e| e.to_string())?;
                        Ok::<AdbClient, String>(client)
                    },
                    |result| match result {
                        Ok(client) => {
                            tracing::info!("adb daemon restarted successfully");
                            Message::AdbReady(Arc::new(client))
                        }
                        Err(e) => Message::DaemonStartFailed(e),
                    },
                )
            }

            Message::OpenUrl(url) => {
                tracing::info!(url = %url, "opening URL in browser");
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&url).spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer").arg(&url).spawn();
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                Task::none()
            }

            // ── Drag-and-Drop ─────────────────────────────────────────────────
            Message::FileHovered => {
                self.file_hover_active = true;
                Task::none()
            }

            Message::FilesHoveredLeft => {
                self.file_hover_active = false;
                Task::none()
            }

            Message::FileDropped(path) => {
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

            // ── File operations — rename ──────────────────────────────────────
            Message::AndroidBeginRename => {
                // Activate rename for the first selected entry
                if let Some(&idx) = self.android_pane.selected.first() {
                    self.android_pane.begin_rename(idx);
                    return text_input::focus(text_input::Id::new(file_pane::RENAME_INPUT_ID));
                }
                Task::none()
            }

            Message::AndroidRenameInput(val) => {
                self.android_pane.update_rename_input(val);
                Task::none()
            }

            Message::AndroidRenameCancel => {
                self.android_pane.cancel_rename();
                Task::none()
            }

            Message::AndroidRenameCommit => {
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

            Message::AndroidRenameComplete => {
                tracing::info!("rename complete — refreshing android listing");
                self.update(Message::AndroidNavigateTo(
                    self.android_pane.current_path.clone(),
                ))
            }

            Message::AndroidRenameFailed(msg) => {
                tracing::warn!(error = %msg, "android rename failed");
                self.update(Message::ShowError(format!("Rename failed: {msg}")))
            }

            // ── File operations — delete ──────────────────────────────────────
            Message::AndroidBeginDelete => {
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

            Message::AndroidDeleteCancel => {
                self.delete_confirm_paths = None;
                Task::none()
            }

            Message::AndroidDeleteConfirm => {
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

            Message::AndroidDeleteComplete => {
                tracing::info!("delete complete — refreshing android listing");
                self.android_pane.selected.clear();
                self.update(Message::AndroidNavigateTo(
                    self.android_pane.current_path.clone(),
                ))
            }

            Message::AndroidDeleteFailed(msg) => {
                tracing::warn!(error = %msg, "android delete failed");
                self.update(Message::ShowError(format!("Delete failed: {msg}")))
            }

            // ── About dialog ──────────────────────────────────────────────────
            Message::OpenAbout => {
                self.about_open = true;
                Task::none()
            }

            Message::CloseAbout => {
                self.about_open = false;
                Task::none()
            }

            // ── Settings ──────────────────────────────────────────────────────
            Message::OpenSettings => {
                self.settings_draft = self.config.clone();
                self.settings_open = true;
                self.preview_modal = None; // close preview if open
                Task::none()
            }

            Message::CloseSettings => {
                self.settings_open = false;
                Task::none()
            }

            Message::SettingsDraftLogLevel(level) => {
                self.settings_draft.log.level = level;
                Task::none()
            }

            Message::SettingsDraftConsole(enabled) => {
                self.settings_draft.log.console_enabled = enabled;
                Task::none()
            }

            Message::SettingsDraftFile(enabled) => {
                self.settings_draft.log.file_enabled = enabled;
                Task::none()
            }

            Message::SaveSettings => {
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

            Message::OpenLogFolder => {
                if let Some(dir) = self.config_path.parent() {
                    let dir = dir.to_string_lossy().into_owned();
                    tracing::info!(dir = %dir, "opening log folder");
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                }
                Task::none()
            }

            // ── Log Viewer ────────────────────────────────────────────────────
            Message::OpenLogViewer => {
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

            Message::CloseLogViewer => {
                self.log_viewer_open = false;
                Task::none()
            }

            Message::LogViewerFilesLoaded(files) => {
                self.log_viewer_files = files;
                // Auto-load the first (newest) file
                if let Some(first) = self.log_viewer_files.first().cloned() {
                    self.update(Message::LogViewerSelectFile(first))
                } else {
                    self.log_viewer_content = "(No log files found)".to_string();
                    Task::none()
                }
            }

            Message::LogViewerSelectFile(path) => {
                self.log_viewer_selected = Some(path.clone());
                Task::perform(
                    async move {
                        std::fs::read_to_string(&path)
                            .unwrap_or_else(|e| format!("(Failed to read log: {e})"))
                    },
                    Message::LogViewerFileLoaded,
                )
            }

            Message::LogViewerFileLoaded(content) => {
                self.log_viewer_content = content;
                Task::none()
            }

            Message::LogViewerSetLevel(level) => {
                self.log_viewer_level = level;
                Task::none()
            }

            Message::ShowToast(msg) => {
                tracing::info!(toast = %msg, "showing info toast");
                self.toast = Some(msg);
                Task::perform(
                    async { tokio::time::sleep(Duration::from_secs(3)).await },
                    |_| Message::DismissToast,
                )
            }

            Message::DismissToast => {
                self.toast = None;
                Task::none()
            }

            // ── Escape routing ────────────────────────────────────────────────
            Message::EscapePressed => {
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

            // ── File preview ──────────────────────────────────────────────────
            Message::PreviewFile(entry) => {
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

            Message::PreviewReady(local_path) => {
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

            Message::PreviewFailed(msg) => {
                tracing::warn!(error = %msg, "file preview pull failed");
                self.update(Message::ShowError(format!("Preview failed: {msg}")))
            }

            Message::ClosePreview => {
                self.preview_modal = None;
                Task::none()
            }
        }
    }
}

// ─── Status derivation ────────────────────────────────────────────────────────

pub(super) fn derive_status(devices: &[AdbDevice]) -> AdbStatus {
    let authorized = devices.iter().find(|d| d.state == DeviceState::Device);
    let unauthorized = devices
        .iter()
        .find(|d| d.state == DeviceState::Unauthorized);

    if let Some(d) = authorized {
        AdbStatus::Connected(d.display_name().to_string())
    } else if unauthorized.is_some() {
        AdbStatus::Unauthorized
    } else {
        AdbStatus::Disconnected
    }
}
