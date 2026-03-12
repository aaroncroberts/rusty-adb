//! rusty-adb — Android File Manager
//!
//! Layout:
//!   ┌─────────────────────────────────────────┐
//!   │              Toolbar (40px)              │
//!   ├──────────────────┬──────────────────────┤
//!   │  Local Files     │   Android Device     │
//!   │  (left pane)     │   (right pane)       │
//!   ├──────────────────┴──────────────────────┤
//!   │              Status Bar (30px)           │
//!   └─────────────────────────────────────────┘
#![allow(mismatched_lifetime_syntaxes)]

mod adb;
mod android_pane;
mod local_pane;
mod status_bar;
mod theme;
mod transfer;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use adb::{AdbClient, AdbDevice, AndroidEntry, DeviceState};
use android_pane::{AndroidPane, AndroidPaneState};
use local_pane::{LocalPane, SortField};
use status_bar::{AdbStatus, StatusBar, TransferStatus};
use theme::ThemeColors;
use transfer::{TransferDirection, TransferEvent, TransferJob};

use iced::widget::{button, column, container, row, text, vertical_rule};
use iced::{Border, Element, Fill, Subscription, Task, Theme};

const TOOLBAR_HEIGHT: f32 = 40.0;

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    // ── ADB ──────────────────────────────────────────────────────────────────
    AdbReady(Arc<AdbClient>),
    PollDevices,
    DevicesLoaded(Arc<Vec<AdbDevice>>),
    AdbError(String),

    // ── Local Pane ────────────────────────────────────────────────────────────
    LocalNavigateTo(PathBuf),
    LocalSelectEntry(usize),
    LocalToggleHidden,
    #[allow(dead_code)] // used in task 3.2
    LocalSortBy(SortField),

    // ── Android Pane ──────────────────────────────────────────────────────────
    AndroidNavigateTo(PathBuf),
    AndroidEntriesLoaded {
        path: PathBuf,
        entries: Arc<Vec<AndroidEntry>>,
        roots: Arc<Vec<PathBuf>>,
    },
    AndroidLoadError(String),
    AndroidSelectEntry(usize),
    /// Drives the loading spinner animation
    SpinnerTick,

    // ── Transfer ──────────────────────────────────────────────────────────────
    /// User pressed "Copy →" (local → android)
    CopyToAndroid,
    /// User pressed "Copy ←" (android → local)
    CopyToLocal,
    /// Progress event from the active transfer subscription
    TransferProgress { percent: u8 },
    /// Transfer completed — carries final speed for display
    TransferComplete { speed_display: String },
    /// Transfer failed
    TransferFailed(String),
}

// ─── App State ────────────────────────────────────────────────────────────────

struct App {
    theme: ThemeColors,
    status_bar: StatusBar,
    adb_status: AdbStatus,
    adb_client: Option<AdbClient>,
    devices: Vec<AdbDevice>,
    /// Serial of the currently active (authorized) device
    active_serial: Option<String>,

    local_pane: LocalPane,
    android_pane: AndroidPane,

    /// Currently active transfer (drives the streaming subscription)
    active_transfer: Option<TransferJob>,
    /// Monotonic counter used to give each transfer a unique subscription ID
    transfer_id: u64,
    /// Live progress shown in the status bar
    transfer_status: Option<TransferStatus>,
}

impl Default for App {
    fn default() -> Self {
        let theme = ThemeColors::dark();
        let start_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self {
            status_bar: StatusBar::new(theme),
            adb_status: AdbStatus::Disconnected,
            adb_client: None,
            devices: Vec::new(),
            active_serial: None,
            local_pane: LocalPane::new(start_path),
            android_pane: AndroidPane::default(),
            active_transfer: None,
            transfer_id: 0,
            transfer_status: None,
            theme,
        }
    }
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

pub fn main() -> iced::Result {
    rusty_logging::LoggingConfig::builder()
        .with_console_compact()
        .with_file_text()
        .with_file_directory("./logs")
        .with_file_prefix("rusty-adb")
        .build()
        .expect("Invalid logging configuration")
        .apply()
        .expect("Failed to initialize logging");

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rusty-adb starting");

    iced::application("rusty-adb", App::update, App::view)
        .subscription(App::subscription)
        .theme(|_| Theme::TokyoNightStorm)
        .window_size((1280.0, 800.0))
        .run_with(|| {
            let init_task = Task::perform(
                async {
                    let client = AdbClient::find().await.map_err(|e| e.to_string())?;
                    let _ = client.start_server().await;
                    Ok::<AdbClient, String>(client)
                },
                |result| match result {
                    Ok(client) => {
                        tracing::info!(adb = %client.adb_path.display(), "adb ready");
                        Message::AdbReady(Arc::new(client))
                    }
                    Err(e) => Message::AdbError(e),
                },
            );
            (App::default(), init_task)
        })
}

// ─── Subscription ─────────────────────────────────────────────────────────────

impl App {
    fn subscription(&self) -> Subscription<Message> {
        let device_poll =
            iced::time::every(Duration::from_secs(2)).map(|_| Message::PollDevices);

        // Spinner — only while the Android pane is loading
        let maybe_spinner = if self.android_pane.state == AndroidPaneState::Loading {
            let spinner =
                iced::time::every(Duration::from_millis(120)).map(|_| Message::SpinnerTick);
            Some(spinner)
        } else {
            None
        };

        // Transfer — stream stderr from the active adb push/pull process.
        // `iced::stream::channel` produces a Stream<Item=Message>; we wrap it
        // with `Subscription::run_with_id` keyed on `job.id` so Iced keeps the
        // same stream alive rather than restarting it on every view pass.
        let maybe_transfer = self.active_transfer.as_ref().map(|job| {
            let job = job.clone();
            Subscription::run_with_id(
                job.id,
                iced::stream::channel(32, move |mut sender| async move {
                    let result = transfer::run_transfer(&job, |event| {
                        let msg = match &event {
                            TransferEvent::Progress { percent } => {
                                Message::TransferProgress { percent: *percent }
                            }
                            TransferEvent::Complete { speed_display } => {
                                Message::TransferComplete {
                                    speed_display: speed_display.clone(),
                                }
                            }
                            TransferEvent::Failed(e) => Message::TransferFailed(e.clone()),
                        };
                        // try_send: channel has capacity 32, ample for ≤100 progress ticks
                        let _ = sender.try_send(msg);
                    })
                    .await;

                    if let Err(e) = result {
                        let _ = sender.try_send(Message::TransferFailed(e.to_string()));
                    }

                    // Park forever — Iced keeps the subscription alive until the
                    // next subscription() call omits this id (i.e., transfer done).
                    std::future::pending::<()>().await;
                    unreachable!()
                }),
            )
        });

        // Combine all active subscriptions
        match (maybe_spinner, maybe_transfer) {
            (Some(s), Some(t)) => Subscription::batch([device_poll, s, t]),
            (Some(s), None) => Subscription::batch([device_poll, s]),
            (None, Some(t)) => Subscription::batch([device_poll, t]),
            (None, None) => device_poll,
        }
    }
}

// ─── Update ───────────────────────────────────────────────────────────────────

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
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
                        // Device just appeared — start loading /sdcard
                        tracing::info!(serial = %serial, "device connected, loading /sdcard");
                        self.active_serial = Some(serial.clone());
                        self.android_pane.on_device_connected();
                        return self.update(Message::AndroidNavigateTo(PathBuf::from("/sdcard")));
                    }
                    (Some(_), None) => {
                        // Device disconnected — cancel any active transfer
                        tracing::info!("device disconnected");
                        self.active_serial = None;
                        self.active_transfer = None;
                        self.transfer_status = None;
                        self.android_pane.on_device_disconnected();
                    }
                    _ => {}
                }

                Task::none()
            }

            Message::AdbError(msg) => {
                tracing::warn!(error = %msg, "adb error");
                self.adb_status = AdbStatus::Error(msg);
                Task::none()
            }

            // ── Local Pane ────────────────────────────────────────────────────
            Message::LocalNavigateTo(path) => {
                self.local_pane.navigate_to(path);
                Task::none()
            }
            Message::LocalSelectEntry(i) => {
                self.local_pane.select(i);
                Task::none()
            }
            Message::LocalToggleHidden => {
                self.local_pane.toggle_hidden();
                Task::none()
            }
            Message::LocalSortBy(field) => {
                self.local_pane.sort_by(field);
                Task::none()
            }

            // ── Android Pane ──────────────────────────────────────────────────
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
                        let (entries_res, roots) =
                            tokio::join!(entries_fut, roots_fut);
                        entries_res
                            .map(|e| (path, e, roots))
                            .map_err(|e| e.to_string())
                    },
                    |result| match result {
                        Ok((path, entries, roots)) => Message::AndroidEntriesLoaded {
                            path,
                            entries: Arc::new(entries),
                            roots: Arc::new(roots),
                        },
                        Err(e) => Message::AndroidLoadError(e),
                    },
                )
            }

            Message::AndroidEntriesLoaded { path, entries, roots } => {
                self.android_pane.on_entries_loaded(
                    path,
                    (*entries).clone(),
                    (*roots).clone(),
                );
                Task::none()
            }

            Message::AndroidLoadError(msg) => {
                tracing::warn!(error = %msg, "android directory load error");
                self.android_pane.on_error(msg);
                Task::none()
            }

            Message::AndroidSelectEntry(i) => {
                self.android_pane.select(i);
                Task::none()
            }

            Message::SpinnerTick => {
                self.android_pane.tick_spinner();
                Task::none()
            }

            // ── Transfer ──────────────────────────────────────────────────────
            Message::CopyToAndroid => {
                let Some(client) = &self.adb_client else {
                    return Task::none();
                };
                let Some(serial) = &self.active_serial else {
                    return Task::none();
                };
                let Some(idx) = self.local_pane.selected else {
                    tracing::warn!("CopyToAndroid: no local file selected");
                    return Task::none();
                };
                let local_entry = &self.local_pane.entries[idx];
                if local_entry.is_dir {
                    tracing::warn!("CopyToAndroid: directory copy not yet supported");
                    return Task::none();
                }

                self.transfer_id += 1;
                let android_dir = self.android_pane.current_path.clone();
                let filename = local_entry.name.clone();

                tracing::info!(
                    file = %local_entry.path.display(),
                    dest = %android_dir.display(),
                    "queuing copy → android"
                );

                let job = TransferJob {
                    id: self.transfer_id,
                    adb_path: client.adb_path.clone(),
                    serial: serial.clone(),
                    source: local_entry.path.clone(),
                    destination: android_dir,
                    direction: TransferDirection::ToAndroid,
                    filename: filename.clone(),
                };

                self.active_transfer = Some(job);
                self.transfer_status = Some(TransferStatus {
                    filename,
                    percent: 0,
                    speed_display: String::new(),
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
                let Some(idx) = self.android_pane.selected else {
                    tracing::warn!("CopyToLocal: no android file selected");
                    return Task::none();
                };
                let android_entry = &self.android_pane.entries[idx];
                if android_entry.is_dir {
                    tracing::warn!("CopyToLocal: directory copy not yet supported");
                    return Task::none();
                }

                self.transfer_id += 1;
                let local_dir = self.local_pane.current_path.clone();
                let filename = android_entry.name.clone();

                tracing::info!(
                    file = %android_entry.path.display(),
                    dest = %local_dir.display(),
                    "queuing copy ← android"
                );

                let job = TransferJob {
                    id: self.transfer_id,
                    adb_path: client.adb_path.clone(),
                    serial: serial.clone(),
                    source: android_entry.path.clone(),
                    destination: local_dir,
                    direction: TransferDirection::ToLocal,
                    filename: filename.clone(),
                };

                self.active_transfer = Some(job);
                self.transfer_status = Some(TransferStatus {
                    filename,
                    percent: 0,
                    speed_display: String::new(),
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
                let direction = self
                    .active_transfer
                    .as_ref()
                    .map(|j| j.direction.clone());

                self.active_transfer = None;
                self.transfer_status = None;

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
                self.adb_status = AdbStatus::Error(format!("Transfer failed: {msg}"));
                Task::none()
            }
        }
    }
}

// ─── Status derivation ────────────────────────────────────────────────────────

fn derive_status(devices: &[AdbDevice]) -> AdbStatus {
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

// ─── View ─────────────────────────────────────────────────────────────────────

impl App {
    fn view(&self) -> Element<Message> {
        column![
            self.view_toolbar(),
            self.view_panes(),
            self.status_bar.view(&self.adb_status, self.transfer_status.as_ref()),
        ]
        .into()
    }

    fn view_toolbar(&self) -> Element<Message> {
        let t = self.theme;
        let label = text("rusty-adb").size(14).color(t.accent);

        // Copy buttons — enabled only when there's an active device and a
        // selected file on the appropriate side.
        let can_copy_to_android = self.active_serial.is_some()
            && self.active_transfer.is_none()
            && self.local_pane.selected
                .and_then(|i| self.local_pane.entries.get(i))
                .map(|e| !e.is_dir)
                .unwrap_or(false);

        let can_copy_to_local = self.active_serial.is_some()
            && self.active_transfer.is_none()
            && self.android_pane.selected
                .and_then(|i| self.android_pane.entries.get(i))
                .map(|e| !e.is_dir)
                .unwrap_or(false);

        let copy_to_android_btn = {
            let lbl = text("Copy →").size(12).color(if can_copy_to_android {
                t.accent
            } else {
                t.text_secondary
            });
            if can_copy_to_android {
                button(lbl).on_press(Message::CopyToAndroid).style(
                    move |_theme, _status| button::Style {
                        background: None,
                        ..Default::default()
                    },
                )
            } else {
                button(lbl).style(move |_theme, _status| button::Style {
                    background: None,
                    ..Default::default()
                })
            }
        };

        let copy_to_local_btn = {
            let lbl = text("Copy ←").size(12).color(if can_copy_to_local {
                t.accent
            } else {
                t.text_secondary
            });
            if can_copy_to_local {
                button(lbl).on_press(Message::CopyToLocal).style(
                    move |_theme, _status| button::Style {
                        background: None,
                        ..Default::default()
                    },
                )
            } else {
                button(lbl).style(move |_theme, _status| button::Style {
                    background: None,
                    ..Default::default()
                })
            }
        };

        let content = row![label, copy_to_android_btn, copy_to_local_btn]
            .spacing(12)
            .padding([0, 16])
            .align_y(iced::Alignment::Center);

        container(content)
            .width(Fill)
            .height(TOOLBAR_HEIGHT)
            .style(move |_theme| container::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn view_panes(&self) -> Element<Message> {
        let left = self.local_pane.view(
            self.theme,
            Message::LocalNavigateTo,
            Message::LocalSelectEntry,
            Message::LocalToggleHidden,
        );

        let right = self.android_pane.view(
            self.theme,
            Message::AndroidNavigateTo,
            Message::AndroidSelectEntry,
        );

        let divider = container(vertical_rule(1))
            .height(Fill)
            .style(move |_theme| container::Style {
                background: Some(self.theme.border.into()),
                ..Default::default()
            });

        row![left, divider, right]
            .width(Fill)
            .height(Fill)
            .into()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn device(serial: &str, state: DeviceState, model: Option<&str>) -> AdbDevice {
        AdbDevice {
            serial: serial.to_string(),
            state,
            model: model.map(str::to_string),
            product: None,
        }
    }

    #[test]
    fn derive_status_empty() {
        assert_eq!(derive_status(&[]), AdbStatus::Disconnected);
    }

    #[test]
    fn derive_status_authorized_device() {
        let devices = vec![device("ABC123", DeviceState::Device, Some("Pixel 7"))];
        assert_eq!(
            derive_status(&devices),
            AdbStatus::Connected("Pixel 7".to_string())
        );
    }

    #[test]
    fn derive_status_authorized_no_model_uses_serial() {
        let devices = vec![device("ABC123", DeviceState::Device, None)];
        assert_eq!(
            derive_status(&devices),
            AdbStatus::Connected("ABC123".to_string())
        );
    }

    #[test]
    fn derive_status_unauthorized() {
        let devices = vec![device("ABC123", DeviceState::Unauthorized, None)];
        assert_eq!(derive_status(&devices), AdbStatus::Unauthorized);
    }

    #[test]
    fn derive_status_prefers_authorized_over_unauthorized() {
        let devices = vec![
            device("ABC123", DeviceState::Unauthorized, None),
            device("DEF456", DeviceState::Device, Some("Galaxy S24")),
        ];
        assert_eq!(
            derive_status(&devices),
            AdbStatus::Connected("Galaxy S24".to_string())
        );
    }

    #[test]
    fn derive_status_offline_is_disconnected() {
        let devices = vec![device("ABC123", DeviceState::Offline, None)];
        assert_eq!(derive_status(&devices), AdbStatus::Disconnected);
    }
}
