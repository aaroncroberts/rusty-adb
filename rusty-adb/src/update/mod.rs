//! Message update handlers for `App`.
use super::{App, Message};
use crate::adb::AdbStatus;
use crate::adb::{AdbDevice, DeviceState};
use crate::fs::{AndroidContext, PaneState};
use iced::Task;
use std::path::PathBuf;
use std::sync::Arc;

mod file_ops;
mod install;
mod pane;
mod queue;
mod transfer;
mod ui;

impl App {
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Pane Layout ───────────────────────────────────────────────────
            Message::ExpandPane(is_android) => self.expand_pane(is_android),
            Message::CollapsePanes => self.collapse_panes(),

            // ── View Modes ────────────────────────────────────────────────────
            Message::SetLocalViewMode(mode) => self.set_local_view_mode(mode),
            Message::SetAndroidViewMode(mode) => self.set_android_view_mode(mode),
            Message::LocalGallerySelect(idx) => self.local_gallery_select(idx),
            Message::AndroidGallerySelect(idx) => self.android_gallery_select(idx),

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

                match detect_device_transition(self.active_serial.as_deref(), &self.devices) {
                    DeviceTransition::Connected { serial, .. } => {
                        tracing::info!(serial = %serial, "device connected, loading /sdcard");
                        let client = self.adb_client.clone().expect("adb_client set at AdbReady");
                        self.active_serial = Some(serial.clone());
                        self.android_ctx = Some(AndroidContext {
                            client,
                            serial: serial.clone(),
                            storage_roots: Vec::new(),
                        });
                        self.android_pane.state = PaneState::Loading;
                        self.update(Message::AndroidNavigateTo(PathBuf::from("/sdcard")))
                    }
                    DeviceTransition::Disconnected => {
                        tracing::info!("device disconnected");
                        self.active_serial = None;
                        self.active_transfer = None;
                        self.transfer_status = None;
                        self.android_ctx = None;
                        self.android_pane.state = PaneState::NoDevice;
                        self.android_pane.entries.clear();
                        self.android_pane.selected.clear();
                        Task::none()
                    }
                    DeviceTransition::NoChange => Task::none(),
                }
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
            Message::LocalNavigateTo(path) => self.local_navigate_to(path),
            Message::LocalEntriesLoaded { path, entries } => {
                self.local_entries_loaded(path, entries)
            }
            Message::LocalLoadError(msg) => self.local_load_error(msg),
            Message::LocalSelectEntry(i) => self.local_select_entry(i),
            Message::ToggleShowPanel(is_android) => {
                // Toggle open/closed; clicking the same pane's button closes it.
                self.show_panel_open = if self.show_panel_open == Some(is_android) {
                    None
                } else {
                    Some(is_android)
                };
                Task::none()
            }
            Message::LocalToggleHidden => {
                self.show_panel_open = None;
                self.local_toggle_hidden()
            }
            Message::LocalToggleType => {
                self.show_panel_open = None;
                self.local_toggle_type()
            }
            Message::LocalToggleSize => {
                self.show_panel_open = None;
                self.local_toggle_size()
            }
            Message::LocalToggleModified => {
                self.show_panel_open = None;
                self.local_toggle_modified()
            }
            Message::LocalSortBy(field) => self.local_sort_by(field),
            Message::AndroidSortBy(field) => self.android_sort_by(field),

            // ── Android Pane ──────────────────────────────────────────────────
            Message::AndroidToggleHidden => {
                self.show_panel_open = None;
                self.android_toggle_hidden()
            }
            Message::AndroidToggleType => {
                self.show_panel_open = None;
                self.android_toggle_type()
            }
            Message::AndroidToggleSize => {
                self.show_panel_open = None;
                self.android_toggle_size()
            }
            Message::AndroidToggleModified => {
                self.show_panel_open = None;
                self.android_toggle_modified()
            }
            Message::AndroidNavigateTo(path) => self.android_navigate_to(path),
            Message::AndroidEntriesLoaded {
                path,
                entries,
                roots,
            } => self.android_entries_loaded(path, entries, roots),
            Message::AndroidLoadError(msg) => self.android_load_error(msg),
            Message::AndroidSelectEntry(i) => self.android_select_entry(i),
            Message::SpinnerTick => self.spinner_tick(),

            // ── Navigation shortcuts ───────────────────────────────────────────
            Message::RefreshPanes => self.refresh_panes(),
            Message::LocalNavigateUp => self.local_navigate_up(),
            Message::DisconnectDevice => self.disconnect_device(),

            // ── Transfer ──────────────────────────────────────────────────────
            Message::CopyToAndroid => self.copy_to_android(),
            Message::CopyToLocal => self.copy_to_local(),
            Message::TransferProgress { percent } => self.transfer_progress(percent),
            Message::TransferComplete { speed_display } => self.transfer_complete(speed_display),
            Message::TransferFailed(msg) => self.transfer_failed(msg),
            Message::CancelTransfer => self.cancel_transfer(),
            Message::TransferCancelled => self.transfer_cancelled(),

            // ── Error / Toast ─────────────────────────────────────────────────
            Message::ShowError(msg) => self.show_error(msg),
            Message::DismissError => self.dismiss_error(),
            Message::ShowToast(msg) => self.show_toast(msg),
            Message::DismissToast => self.dismiss_toast(),

            // ── ADB install flow ──────────────────────────────────────────────
            Message::AdbNotFound => self.adb_not_found(),
            Message::RetryAdbFind => self.retry_adb_find(),
            Message::InstallAdb => self.install_adb(),
            Message::InstallComplete => self.install_complete(),
            Message::InstallFailed(err) => self.install_failed(err),
            Message::DaemonStartFailed(msg) => self.daemon_start_failed(msg),
            Message::RestartDaemon => self.restart_daemon(),
            Message::OpenUrl(url) => self.open_url(url),

            // ── Drag-and-Drop ─────────────────────────────────────────────────
            Message::FileHovered => self.file_hovered(),
            Message::FilesHoveredLeft => self.files_hovered_left(),
            Message::FileDropped(path) => self.file_dropped(path),
            Message::LocalDragStarted => self.local_drag_started(),
            Message::DroppedOnAndroid => self.dropped_on_android(),
            Message::DragCancelled => self.drag_cancelled(),

            // ── File operations — rename ──────────────────────────────────────
            Message::AndroidBeginRename => self.android_begin_rename(),
            Message::AndroidRenameInput(val) => self.android_rename_input(val),
            Message::AndroidRenameCancel => self.android_rename_cancel(),
            Message::AndroidRenameCommit => self.android_rename_commit(),
            Message::AndroidRenameComplete => self.android_rename_complete(),
            Message::AndroidRenameFailed(msg) => self.android_rename_failed(msg),

            // ── File operations — delete ──────────────────────────────────────
            Message::AndroidBeginDelete => self.android_begin_delete(),
            Message::AndroidDeleteCancel => self.android_delete_cancel(),
            Message::AndroidDeleteConfirm => self.android_delete_confirm(),
            Message::AndroidDeleteComplete => self.android_delete_complete(),
            Message::AndroidDeleteFailed(msg) => self.android_delete_failed(msg),

            // ── Copy Queue ────────────────────────────────────────────────────
            Message::CloseCopyConfirm => self.close_copy_confirm(),
            Message::ConfirmCopyToDevice => self.confirm_copy_to_device(),
            Message::OpenQueueDialog => self.open_queue_dialog(),
            Message::CloseQueueDialog => self.close_queue_dialog(),
            Message::ToggleQueuePause => self.toggle_queue_pause(),
            Message::QueueRemoveItem(id) => self.queue_remove_item(id),
            Message::QueueEditItem(id) => self.queue_edit_item(id),
            Message::QueueEditDestInput(val) => self.queue_edit_dest_input(val),
            Message::QueueEditDestConfirm => self.queue_edit_dest_confirm(),
            Message::QueueItemComplete(id) => self.queue_item_complete(id),
            Message::QueueItemFailed { id, reason } => self.queue_item_failed(id, reason),

            // ── Device Details dialog ─────────────────────────────────────────
            Message::OpenDeviceDetails => self.open_device_details(),
            Message::CloseDeviceDetails => self.close_device_details(),
            Message::RefreshDeviceDetails => self.refresh_device_details(),
            Message::DeviceDetailsLoaded(details) => self.device_details_loaded(details),
            Message::DeviceDetailsFailed(e) => self.device_details_failed(e),
            Message::DeviceDetailsSelectTab(tab) => self.device_details_select_tab(tab),

            // ── About dialog ──────────────────────────────────────────────────
            Message::OpenAbout => self.open_about(),
            Message::CloseAbout => self.close_about(),

            // ── Settings ──────────────────────────────────────────────────────
            Message::OpenSettings => self.open_settings(),
            Message::CloseSettings => self.close_settings(),
            Message::SettingsDraftLogLevel(level) => self.settings_draft_log_level(level),
            Message::SettingsDraftConsole(enabled) => self.settings_draft_console(enabled),
            Message::SettingsDraftFile(enabled) => self.settings_draft_file(enabled),
            Message::SaveSettings => self.save_settings(),
            Message::OpenLogFolder => self.open_log_folder(),

            // ── Log Viewer ────────────────────────────────────────────────────
            Message::OpenLogViewer => self.open_log_viewer(),
            Message::CloseLogViewer => self.close_log_viewer(),
            Message::LogViewerFilesLoaded(files) => self.log_viewer_files_loaded(files),
            Message::LogViewerSelectFile(path) => self.log_viewer_select_file(path),
            Message::LogViewerFileLoaded(content) => self.log_viewer_file_loaded(content),
            Message::LogViewerSetLevel(level) => self.log_viewer_set_level(level),

            // ── Escape routing ────────────────────────────────────────────────
            Message::EscapePressed => self.escape_pressed(),

            // ── File preview ──────────────────────────────────────────────────
            Message::PreviewFile(entry) => self.preview_file(entry),
            Message::PreviewReady(local_path) => self.preview_ready(local_path),
            Message::PreviewFailed(msg) => self.preview_failed(msg),
            Message::ClosePreview => self.close_preview(),
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

// ─── Device transition detection ─────────────────────────────────────────────

/// What changed in the device list since the last poll.
///
/// Returned by [`detect_device_transition`] — a pure function that can be
/// unit-tested without constructing a full [`App`].
#[derive(Debug, PartialEq)]
pub(crate) enum DeviceTransition {
    /// A new authorized device appeared.  Contains the serial and a
    /// human-readable display label (model name or serial as fallback).
    Connected { serial: String, label: String },
    /// The previously active device is no longer in the authorized list.
    Disconnected,
    /// No actionable change (same device, still connected; or still no device).
    NoChange,
}

/// Pure function: compute whether the active device has changed.
///
/// Returns [`DeviceTransition::Connected`] when a new authorized device
/// appears, [`DeviceTransition::Disconnected`] when the active one disappears,
/// or [`DeviceTransition::NoChange`] for all other cases.
pub(crate) fn detect_device_transition(
    active: Option<&str>,
    devices: &[AdbDevice],
) -> DeviceTransition {
    let authorized = devices.iter().find(|d| d.state == DeviceState::Device);
    let new_serial = authorized.map(|d| d.serial.as_str());

    match (active, new_serial) {
        (None, Some(serial)) => {
            let label = authorized
                .and_then(|d| d.model.as_deref())
                .unwrap_or(serial)
                .to_string();
            DeviceTransition::Connected {
                serial: serial.to_string(),
                label,
            }
        }
        (Some(_), None) => DeviceTransition::Disconnected,
        _ => DeviceTransition::NoChange,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(serial: &str, state: DeviceState, model: Option<&str>) -> AdbDevice {
        AdbDevice {
            serial: serial.to_string(),
            state,
            model: model.map(String::from),
            product: None,
        }
    }

    // ── detect_device_transition ──────────────────────────────────────────────

    #[test]
    fn transition_none_to_authorized_with_model() {
        let devices = vec![dev("ABC123", DeviceState::Device, Some("Pixel 7"))];
        assert_eq!(
            detect_device_transition(None, &devices),
            DeviceTransition::Connected {
                serial: "ABC123".into(),
                label: "Pixel 7".into()
            }
        );
    }

    #[test]
    fn transition_none_to_authorized_no_model_uses_serial() {
        let devices = vec![dev("emulator-5554", DeviceState::Device, None)];
        assert_eq!(
            detect_device_transition(None, &devices),
            DeviceTransition::Connected {
                serial: "emulator-5554".into(),
                label: "emulator-5554".into()
            }
        );
    }

    #[test]
    fn transition_some_to_none_is_disconnected() {
        assert_eq!(
            detect_device_transition(Some("ABC123"), &[]),
            DeviceTransition::Disconnected
        );
    }

    #[test]
    fn transition_same_device_is_no_change() {
        let devices = vec![dev("ABC123", DeviceState::Device, None)];
        assert_eq!(
            detect_device_transition(Some("ABC123"), &devices),
            DeviceTransition::NoChange
        );
    }

    #[test]
    fn transition_no_device_before_or_after_is_no_change() {
        assert_eq!(
            detect_device_transition(None, &[]),
            DeviceTransition::NoChange
        );
    }

    #[test]
    fn transition_unauthorized_only_is_no_change() {
        let devices = vec![dev("ABC123", DeviceState::Unauthorized, None)];
        assert_eq!(
            detect_device_transition(None, &devices),
            DeviceTransition::NoChange
        );
    }

    // ── derive_status ─────────────────────────────────────────────────────────

    #[test]
    fn status_authorized_device_uses_model() {
        let devices = vec![dev("S1", DeviceState::Device, Some("Pixel 7"))];
        assert_eq!(
            derive_status(&devices),
            AdbStatus::Connected("Pixel 7".into())
        );
    }

    #[test]
    fn status_unauthorized_device() {
        let devices = vec![dev("S1", DeviceState::Unauthorized, None)];
        assert_eq!(derive_status(&devices), AdbStatus::Unauthorized);
    }

    #[test]
    fn status_empty_is_disconnected() {
        assert_eq!(derive_status(&[]), AdbStatus::Disconnected);
    }
}
