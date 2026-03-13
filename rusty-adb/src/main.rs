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
mod config;
mod local_pane;
mod status_bar;
mod theme;
mod transfer;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use adb::{AdbClient, AdbDevice, AndroidEntry, DeviceState};
use android_pane::{AndroidPane, AndroidPaneState};
use local_pane::{LocalPane, SortField};
use status_bar::{AdbStatus, StatusBar, TransferStatus};
use theme::ThemeColors;
use transfer::{TransferDirection, TransferEvent, TransferJob};

use iced::keyboard::{self, key::Named};
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

    // ── Navigation ────────────────────────────────────────────────────────────
    /// F5: refresh local entries + reload android directory
    RefreshPanes,
    /// Backspace: navigate up one level in the local pane
    LocalNavigateUp,
    /// Disconnect the currently active device (adb disconnect SERIAL)
    DisconnectDevice,

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
    /// User pressed the Cancel button during a transfer
    CancelTransfer,
    /// Transfer was cancelled (emitted by the subscription)
    TransferCancelled,
    /// Show a transient error banner (auto-dismisses after 3 s)
    ShowError(String),
    /// Dismiss the error banner (fired by the 3-second Task)
    DismissError,

    // ── ADB install flow ──────────────────────────────────────────────────────
    /// ADB binary not found — show the install guide screen
    AdbNotFound,
    /// User clicked "Retry" on the install screen — re-run AdbClient::find()
    RetryAdbFind,
    /// User clicked a platform package manager install button
    /// (actual implementation wired in task 3m2.1.2)
    InstallAdb,
    /// Open a URL in the system default browser
    OpenUrl(String),
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
    /// Pending transfers waiting behind the active one
    transfer_queue: VecDeque<TransferJob>,
    /// Shared cancel flag — write `true` to abort the active transfer
    cancel_flag: Option<Arc<AtomicBool>>,
    /// How many jobs have completed in the current batch (for queue display)
    transfer_queue_done: usize,
    /// Total jobs in the current batch (for queue display)
    transfer_queue_total: usize,
    /// Transient error message shown in a dismissible banner (None = hidden)
    error_banner: Option<String>,

    /// Lines of output from an in-progress package manager install
    install_log: Vec<String>,
    /// Whether a package manager install is currently running
    installing: bool,
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
            transfer_queue: VecDeque::new(),
            cancel_flag: None,
            transfer_queue_done: 0,
            transfer_queue_total: 0,
            error_banner: None,
            install_log: Vec::new(),
            installing: false,
            theme,
        }
    }
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

pub fn main() -> iced::Result {
    // Resolve ~/.rusty-adb/ and ensure it exists before logging starts.
    let app_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rusty-adb");
    std::fs::create_dir_all(&app_dir).ok();

    // Load config.yml (silently uses defaults if absent or unreadable).
    let cfg = config::AppConfig::load(&app_dir.join("config.yml"));

    // Build logging from config values.
    let mut log_builder = rusty_logging::LoggingConfig::builder()
        .with_filter(&cfg.log.level)
        .with_rotation_policy(rusty_logging::RotationPolicy::Daily)
        .with_file_directory(&app_dir)
        .with_file_prefix("rusty-adb");

    log_builder = if cfg.log.console_enabled {
        log_builder.with_console_compact()
    } else {
        log_builder.without_console()
    };

    log_builder = if cfg.log.file_enabled {
        log_builder.with_file_text()
    } else {
        log_builder.without_file()
    };

    log_builder
        .build()
        .expect("Invalid logging configuration")
        .apply()
        .expect("Failed to initialize logging");

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rusty-adb starting");

    iced::application(
        |app: &App| match &app.adb_status {
            AdbStatus::Connected(name) => format!("rusty-adb — {name}"),
            _ => "rusty-adb".to_string(),
        },
        App::update,
        App::view,
    )
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
                    Err(e) => {
                        // "adb not found" is a known, actionable state — show install UI
                        if e.starts_with("adb not found") {
                            tracing::warn!("adb binary not found — showing install guide");
                            Message::AdbNotFound
                        } else {
                            Message::AdbError(e)
                        }
                    }
                },
            );
            (App::default(), init_task)
        })
}

// ─── Keyboard ─────────────────────────────────────────────────────────────────

/// Map key presses to messages. Called by the keyboard subscription.
/// Returns `None` to ignore keys not bound to an action.
fn handle_key_press(key: keyboard::Key, _mods: keyboard::Modifiers) -> Option<Message> {
    match key.as_ref() {
        keyboard::Key::Named(Named::F5) => Some(Message::RefreshPanes),
        keyboard::Key::Named(Named::Backspace) => Some(Message::LocalNavigateUp),
        _ => None,
    }
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
            let cancel = self
                .cancel_flag
                .clone()
                .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            Subscription::run_with_id(
                job.id,
                iced::stream::channel(32, move |mut sender| async move {
                    let result = transfer::run_transfer(&job, cancel, |event| {
                        let msg = match &event {
                            TransferEvent::Progress { percent } => {
                                Message::TransferProgress { percent: *percent }
                            }
                            TransferEvent::Complete { speed_display } => {
                                Message::TransferComplete {
                                    speed_display: speed_display.clone(),
                                }
                            }
                            TransferEvent::Cancelled => Message::TransferCancelled,
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

        // Keyboard shortcuts — F5 refresh, Backspace navigate up
        let keys = keyboard::on_key_press(handle_key_press);

        // Combine all active subscriptions
        let mut subs = vec![device_poll, keys];
        if let Some(s) = maybe_spinner {
            subs.push(s);
        }
        if let Some(t) = maybe_transfer {
            subs.push(t);
        }
        Subscription::batch(subs)
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
                self.adb_status = AdbStatus::Error(msg.clone());
                self.update(Message::ShowError(msg))
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

            // ── Navigation shortcuts ───────────────────────────────────────────
            Message::RefreshPanes => {
                // Reload local entries in-place
                self.local_pane.reload();
                // Re-fetch the current android directory
                let path = self.android_pane.current_path.clone();
                if self.active_serial.is_some() {
                    return self.update(Message::AndroidNavigateTo(path));
                }
                Task::none()
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
                let first = jobs.pop_front().unwrap();
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
                let first = jobs.pop_front().unwrap();
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
                let direction = self
                    .active_transfer
                    .as_ref()
                    .map(|j| j.direction.clone());

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
                return self.update(Message::ShowError(format!("Transfer failed: {msg}")));
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
                        let _ = client.start_server().await;
                        Ok::<AdbClient, String>(client)
                    },
                    |result| match result {
                        Ok(client) => {
                            tracing::info!(adb = %client.adb_path.display(), "adb found on retry");
                            Message::AdbReady(Arc::new(client))
                        }
                        Err(e) => {
                            if e.starts_with("adb not found") {
                                Message::AdbNotFound
                            } else {
                                Message::AdbError(e)
                            }
                        }
                    },
                )
            }

            // Actual install logic wired in task 3m2.1.2
            Message::InstallAdb => Task::none(),

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
        // ADB not installed — replace panes with the setup guide
        if self.adb_status == AdbStatus::NotFound {
            let mut items: Vec<Element<Message>> = vec![self.view_toolbar()];
            items.push(self.view_adb_not_found());
            items.push(self.status_bar.view(&self.adb_status, None, None));
            return column(items).into();
        }

        let mut items: Vec<Element<Message>> = vec![self.view_toolbar()];
        if let Some(msg) = &self.error_banner {
            items.push(self.view_error_banner(msg));
        }
        items.push(self.view_panes());
        items.push(self.status_bar.view(
            &self.adb_status,
            self.transfer_status.as_ref(),
            self.active_transfer.as_ref().map(|_| Message::CancelTransfer),
        ));
        column(items).into()
    }

    fn view_adb_not_found(&self) -> Element<Message> {
        use iced::widget::{scrollable, Space};
        use iced::Alignment;

        let t = self.theme;

        // ── Install button (platform-specific) ───────────────────────────────
        #[cfg(target_os = "macos")]
        let install_label = "Install via Homebrew";
        #[cfg(target_os = "windows")]
        let install_label = "Install via winget";
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let install_label = "Download Platform Tools";

        let install_btn = button(
            text(install_label).size(13).color(iced::Color::WHITE),
        )
        .style(move |_t, _s| button::Style {
            background: Some(t.accent.into()),
            border: Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        })
        .padding([8, 16])
        .on_press(Message::InstallAdb);

        let retry_btn = button(text("↺  Retry Detection").size(13).color(t.text))
            .style(move |_t, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .padding([8, 16])
            .on_press(Message::RetryAdbFind);

        let download_btn = button(text("⬇  Download Manually").size(13).color(t.accent))
            .style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .on_press(Message::OpenUrl(
                "https://developer.android.com/tools/releases/platform-tools".to_string(),
            ));

        // ── Install log output (shown during/after install attempt) ──────────
        let log_section: Element<Message> = if self.install_log.is_empty() {
            Space::with_height(0).into()
        } else {
            let log_text = self.install_log.join("\n");
            scrollable(
                container(text(log_text).size(11).color(t.text_secondary).font(
                    iced::Font::with_name("Menlo"),
                ))
                .width(Fill)
                .padding([8, 12])
                .style(move |_t| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border { color: t.border, width: 1.0, radius: 4.0.into() },
                    ..Default::default()
                }),
            )
            .height(120)
            .into()
        };

        // ── Platform-specific install steps ──────────────────────────────────
        #[cfg(target_os = "macos")]
        let steps: Element<Message> = column![
            text("Option A — Homebrew (recommended)").size(12).color(t.text_secondary),
            text("  brew install --cask android-platform-tools")
                .size(12)
                .color(t.accent)
                .font(iced::Font::with_name("Menlo")),
            Space::with_height(8),
            text("Option B — Android Studio").size(12).color(t.text_secondary),
            text("  Open SDK Manager → SDK Tools → Android SDK Platform-Tools")
                .size(12)
                .color(t.text_secondary),
            text("  SDK installs to ~/Library/Android/sdk/platform-tools/")
                .size(11)
                .color(t.text_secondary),
        ]
        .spacing(4)
        .into();

        #[cfg(target_os = "windows")]
        let steps: Element<Message> = column![
            text("Option A — winget (recommended)").size(12).color(t.text_secondary),
            text("  winget install Google.PlatformTools")
                .size(12)
                .color(t.accent)
                .font(iced::Font::with_name("Menlo")),
            Space::with_height(8),
            text("Option B — Android Studio").size(12).color(t.text_secondary),
            text("  Open SDK Manager → SDK Tools → Android SDK Platform-Tools")
                .size(12)
                .color(t.text_secondary),
            text("  SDK installs to %LOCALAPPDATA%\\Android\\Sdk\\platform-tools\\")
                .size(11)
                .color(t.text_secondary),
        ]
        .spacing(4)
        .into();

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let steps: Element<Message> = column![
            text("Download from developer.android.com/tools/releases/platform-tools")
                .size(12)
                .color(t.text_secondary),
            text("  Extract and ensure 'adb' is on your PATH")
                .size(12)
                .color(t.text_secondary),
        ]
        .spacing(4)
        .into();

        // ── Main card layout ─────────────────────────────────────────────────
        let card = container(
            column![
                text("ADB Not Found").size(24).color(t.text),
                Space::with_height(8),
                text("Android Debug Bridge (ADB) is required to connect to your device.")
                    .size(13)
                    .color(t.text_secondary),
                Space::with_height(20),
                steps,
                Space::with_height(20),
                row![install_btn, retry_btn, download_btn].spacing(12),
                Space::with_height(12),
                log_section,
            ]
            .spacing(0)
            .width(520),
        )
        .padding(32)
        .style(move |_t| container::Style {
            background: Some(t.background_secondary.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

        container(card)
            .width(Fill)
            .height(Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    fn view_error_banner<'a>(&'a self, msg: &'a str) -> Element<'a, Message> {
        let t = self.theme;
        let content = row![
            text(format!("⚠  {msg}")).size(12).color(t.error),
            iced::widget::Space::with_width(Fill),
            button(text("✕").size(11).color(t.error))
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(Message::DismissError),
        ]
        .align_y(iced::Alignment::Center)
        .padding([4, 12]);

        container(content)
            .width(Fill)
            .style(move |_t| container::Style {
                background: Some(t.error.scale_alpha(0.12).into()),
                border: Border {
                    color: t.error.scale_alpha(0.4),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn view_toolbar(&self) -> Element<Message> {
        let t = self.theme;

        // ── Availability flags ───────────────────────────────────────────────
        let has_device = self.active_serial.is_some();
        let no_transfer = self.active_transfer.is_none();

        let can_copy_to_android = has_device
            && no_transfer
            && self.local_pane.selected.iter().any(|&i| {
                self.local_pane
                    .entries
                    .get(i)
                    .map(|e| !e.is_dir)
                    .unwrap_or(false)
            });

        let can_copy_to_local = has_device
            && no_transfer
            && self.android_pane.selected.iter().any(|&i| {
                self.android_pane
                    .entries
                    .get(i)
                    .map(|e| !e.is_dir)
                    .unwrap_or(false)
            });

        // ── Button builder helper ─────────────────────────────────────────────
        let toolbar_btn = move |label: String, color: iced::Color, msg: Option<Message>| {
            let lbl = text(label).size(12).color(color);
            let btn = button(lbl).style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            });
            if let Some(m) = msg {
                btn.on_press(m)
            } else {
                btn
            }
        };

        // ── Buttons ───────────────────────────────────────────────────────────
        let refresh_btn = toolbar_btn(
            "⟳ Refresh".to_string(),
            t.text,
            Some(Message::RefreshPanes),
        );

        let disconnect_btn = toolbar_btn(
            "Disconnect".to_string(),
            if has_device { t.warning } else { t.text_secondary },
            if has_device { Some(Message::DisconnectDevice) } else { None },
        );

        let copy_to_android_btn = toolbar_btn(
            "Copy →".to_string(),
            if can_copy_to_android { t.accent } else { t.text_secondary },
            if can_copy_to_android { Some(Message::CopyToAndroid) } else { None },
        );

        let copy_to_local_btn = toolbar_btn(
            "Copy ←".to_string(),
            if can_copy_to_local { t.accent } else { t.text_secondary },
            if can_copy_to_local { Some(Message::CopyToLocal) } else { None },
        );

        let content = row![
            refresh_btn,
            disconnect_btn,
            copy_to_android_btn,
            copy_to_local_btn,
        ]
        .spacing(8)
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
            Message::LocalSortBy,
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
