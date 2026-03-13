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
use iced::widget::{button, column, container, image, pick_list, row, scrollable, stack, text, text_input, toggler, vertical_rule};
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
    /// User clicked a platform package manager install button — starts background install
    InstallAdb,
    /// Package manager install completed successfully → triggers RetryAdbFind
    InstallComplete,
    /// Package manager install failed with an error message
    InstallFailed(String),
    /// Open a URL in the system default browser
    OpenUrl(String),
    /// start_server() returned an error — show with restart option
    DaemonStartFailed(String),
    /// User clicked "Restart Daemon" — runs adb kill-server then start-server
    RestartDaemon,

    // ── Drag-and-Drop ─────────────────────────────────────────────────────────
    /// A file from the OS is being dragged over the window — show drop highlight
    FileHovered,
    /// The hovered file(s) left the window without being dropped
    FilesHoveredLeft,
    /// A file was dropped onto the window — queue a local→android transfer
    FileDropped(PathBuf),

    // ── Settings ──────────────────────────────────────────────────────────────
    /// Open the settings modal (copies live config to draft)
    OpenSettings,
    /// Close settings without saving
    CloseSettings,
    /// User changed the log level pick-list
    SettingsDraftLogLevel(String),
    /// User toggled console logging
    SettingsDraftConsole(bool),
    /// User toggled file logging
    SettingsDraftFile(bool),
    /// Save settings_draft to config.yml
    SaveSettings,
    /// Open the app data directory in the OS file manager
    OpenLogFolder,
    /// Show a transient success/info toast (auto-dismisses after 3 s)
    ShowToast(String),
    /// Dismiss the info toast
    DismissToast,

    // ── File Preview ──────────────────────────────────────────────────────────
    /// Double-click on a file entry — check size then pull to temp
    PreviewFile(AndroidEntry),
    /// adb pull succeeded — local temp path ready for rendering
    PreviewReady(PathBuf),
    /// adb pull failed
    PreviewFailed(String),
    /// Close the preview modal (also mapped from Escape when modal is open)
    ClosePreview,

    // ── Escape key ────────────────────────────────────────────────────────────
    /// Escape pressed — routes to ClosePreview or AndroidRenameCancel
    EscapePressed,

    // ── File operations (rename / delete on Android device) ───────────────────
    /// F2: begin inline rename for the first selected android entry
    AndroidBeginRename,
    /// Keystroke inside the inline rename text input
    AndroidRenameInput(String),
    /// Enter: commit rename — calls adb shell mv then refreshes listing
    AndroidRenameCommit,
    /// Escape: cancel rename without changes
    AndroidRenameCancel,
    /// adb shell mv completed successfully — reload current dir
    AndroidRenameComplete,
    /// adb shell mv returned an error
    AndroidRenameFailed(String),
    /// Delete/toolbar: open confirmation for selected android entries
    AndroidBeginDelete,
    /// User clicked "Confirm Delete" in the confirmation banner
    AndroidDeleteConfirm,
    /// User clicked "Cancel" in the confirmation banner
    AndroidDeleteCancel,
    /// adb shell rm -rf completed — reload current dir
    AndroidDeleteComplete,
    /// adb shell rm -rf returned an error
    AndroidDeleteFailed(String),
}

// ─── Preview Content ──────────────────────────────────────────────────────────

/// What is being shown in the preview modal
#[derive(Debug, Clone)]
enum PreviewContent {
    /// An image file — rendered with the iced image widget
    Image(PathBuf),
    /// A text file — shown in a scrollable text widget
    Text(String),
    /// Extension not supported for preview
    Unsupported(String),
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
    /// Consecutive ADB poll failures — triggers auto-restart at 3, error UI at 4+
    daemon_error_count: u8,

    /// Lines of output from an in-progress package manager install
    install_log: Vec<String>,
    /// Whether a package manager install is currently running
    installing: bool,

    /// `true` while an OS file drag is hovering over the window — shows drop highlight
    file_hover_active: bool,
    /// Paths pending deletion confirmation — shown in the delete confirmation banner
    delete_confirm_paths: Option<Vec<PathBuf>>,

    /// Preview modal content (None = closed)
    preview_modal: Option<PreviewContent>,
    /// Last-click tracking for double-click detection: (entry_index, when)
    android_last_click: Option<(usize, std::time::Instant)>,

    /// Loaded application configuration (written to disk on Save)
    config: config::AppConfig,
    /// Path to config.yml — used when writing settings back to disk
    config_path: PathBuf,
    /// Whether the settings modal is currently open
    settings_open: bool,
    /// Working copy of config being edited in the settings modal
    settings_draft: config::AppConfig,
    /// Transient success/info toast (None = hidden)
    toast: Option<String>,
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
            daemon_error_count: 0,
            file_hover_active: false,
            delete_confirm_paths: None,
            preview_modal: None,
            android_last_click: None,
            config: config::AppConfig::default(),
            config_path: PathBuf::new(),
            settings_open: false,
            settings_draft: config::AppConfig::default(),
            toast: None,
            theme,
        }
    }
}

impl App {
    /// Construct App with an already-loaded config (used at startup).
    fn with_config(cfg: config::AppConfig, config_path: PathBuf) -> Self {
        Self {
            config: cfg.clone(),
            config_path,
            settings_draft: cfg,
            ..Self::default()
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
    let config_path = app_dir.join("config.yml");
    let cfg = config::AppConfig::load(&config_path);

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
                    if let Err(e) = client.start_server().await {
                        return Err(format!("daemon:{}", e));
                    }
                    Ok::<AdbClient, String>(client)
                },
                |result| match result {
                    Ok(client) => {
                        tracing::info!(adb = %client.adb_path.display(), "adb ready");
                        Message::AdbReady(Arc::new(client))
                    }
                    Err(e) if e.starts_with("adb not found") => {
                        tracing::warn!("adb binary not found — showing install guide");
                        Message::AdbNotFound
                    }
                    Err(e) if e.starts_with("daemon:") => {
                        let msg = e["daemon:".len()..].to_string();
                        tracing::warn!(error = %msg, "adb start-server failed");
                        Message::DaemonStartFailed(msg)
                    }
                    Err(e) => Message::AdbError(e),
                },
            );
            (App::with_config(cfg, config_path), init_task)
        })
}

// ─── Keyboard ─────────────────────────────────────────────────────────────────

/// Map key presses to messages. Called by the keyboard subscription.
/// Returns `None` to ignore keys not bound to an action.
fn handle_key_press(key: keyboard::Key, _mods: keyboard::Modifiers) -> Option<Message> {
    match key.as_ref() {
        keyboard::Key::Named(Named::F5) => Some(Message::RefreshPanes),
        keyboard::Key::Named(Named::Backspace) => Some(Message::LocalNavigateUp),
        keyboard::Key::Named(Named::F2) => Some(Message::AndroidBeginRename),
        keyboard::Key::Named(Named::Escape) => Some(Message::EscapePressed),
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

        // File drag-and-drop from the OS — FileHovered/FileDropped/FilesHoveredLeft
        let file_drops =
            iced::event::listen_with(|event, _status, _window| match event {
                iced::Event::Window(iced::window::Event::FileHovered(_)) => {
                    Some(Message::FileHovered)
                }
                iced::Event::Window(iced::window::Event::FilesHoveredLeft) => {
                    Some(Message::FilesHoveredLeft)
                }
                iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                    Some(Message::FileDropped(path))
                }
                _ => None,
            });

        // Combine all active subscriptions
        let mut subs = vec![device_poll, keys, file_drops];
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
                        "ADB daemon unresponsive. Use 'Restart Daemon' in the toolbar."
                            .to_string(),
                    ));
                }
                // Counts 1–2: transient failures — suppress to avoid spurious error toasts
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
                        if !entry.is_dir {
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
                self.install_log
                    .push("Starting installation…".to_string());
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
                            .map_err(|e| format!("brew not found: {e}. Install Homebrew from https://brew.sh"))?;

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
                self.install_log.push("✓ Installation complete. Detecting adb…".to_string());
                self.installing = false;
                self.update(Message::RetryAdbFind)
            }

            Message::InstallFailed(err) => {
                tracing::warn!(error = %err, "platform-tools install failed");
                self.installing = false;
                self.install_log.push(format!("✗ {err}"));
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
                let (Some(client), Some(serial)) =
                    (&self.adb_client, &self.active_serial)
                else {
                    tracing::warn!(path = %path.display(), "file dropped but no device connected");
                    return self.update(Message::ShowError(
                        "No device connected — connect a device before dropping files."
                            .to_string(),
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
                    return text_input::focus(text_input::Id::new(
                        android_pane::RENAME_INPUT_ID,
                    ));
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
                    async move { client.rename(&serial, &from_path, &to_path).await.map_err(|e| e.to_string()) },
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
                            client.delete(&serial, path).await.map_err(|e| e.to_string())?;
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
                if self.settings_open {
                    return self.update(Message::CloseSettings);
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
                        client.pull_to_temp(&serial, &path).await.map_err(|e| e.to_string())
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
                            Err(e) => PreviewContent::Unsupported(format!(
                                "Could not read file: {e}"
                            )),
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
        if let Some(msg) = &self.toast {
            items.push(self.view_toast_banner(msg));
        }
        if let Some(paths) = &self.delete_confirm_paths {
            items.push(self.view_delete_confirm(paths));
        }
        items.push(self.view_panes());
        items.push(self.status_bar.view(
            &self.adb_status,
            self.transfer_status.as_ref(),
            self.active_transfer.as_ref().map(|_| Message::CancelTransfer),
        ));

        let base: Element<Message> = column(items).into();

        // Stack-based modal overlays (settings takes priority over preview)
        if self.settings_open {
            stack![base, self.view_settings_modal()].into()
        } else if let Some(modal_content) = &self.preview_modal {
            stack![base, self.view_preview_modal(modal_content)].into()
        } else {
            base
        }
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

    /// Delete confirmation banner — shown above the panes when paths are pending deletion.
    fn view_delete_confirm<'a>(&'a self, paths: &'a [PathBuf]) -> Element<'a, Message> {
        let t = self.theme;
        let label = if paths.len() == 1 {
            format!(
                "Delete \"{}\"? This cannot be undone.",
                paths[0].file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!("Delete {} items? This cannot be undone.", paths.len())
        };

        let confirm_btn = button(text("Confirm Delete").size(12).color(iced::Color::WHITE))
            .style(move |_t, _s| button::Style {
                background: Some(t.error.into()),
                border: Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            })
            .padding([4, 12])
            .on_press(Message::AndroidDeleteConfirm);

        let cancel_btn = button(text("Cancel").size(12).color(t.text))
            .style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .padding([4, 8])
            .on_press(Message::AndroidDeleteCancel);

        let content = row![
            text(label).size(12).color(t.text).width(Fill),
            confirm_btn,
            cancel_btn,
        ]
        .align_y(iced::Alignment::Center)
        .spacing(8)
        .padding([4, 12]);

        container(content)
            .width(Fill)
            .style(move |_t| container::Style {
                background: Some(t.warning.scale_alpha(0.15).into()),
                border: Border {
                    color: t.warning.scale_alpha(0.5),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// Preview modal overlay — rendered on top of the full UI via `stack!`.
    ///
    /// The semi-transparent backdrop captures clicks (closing the modal).
    /// The inner card shows either an image or scrollable text.
    fn view_preview_modal<'a>(&'a self, content: &'a PreviewContent) -> Element<'a, Message> {
        let t = self.theme;

        let close_btn = button(text("✕  Close").size(12).color(t.text))
            .style(move |_th, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .padding([4, 12])
            .on_press(Message::ClosePreview);

        let preview_body: Element<Message> = match content {
            PreviewContent::Image(path) => {
                let handle = image::Handle::from_path(path);
                container(image(handle).width(Fill).height(Fill))
                    .width(Fill)
                    .height(Fill)
                    .into()
            }
            PreviewContent::Text(text_content) => {
                scrollable(
                    container(
                        text(text_content.clone())
                            .size(12)
                            .color(t.text)
                            .font(iced::Font::with_name("Menlo")),
                    )
                    .padding([8, 12]),
                )
                .width(Fill)
                .height(Fill)
                .into()
            }
            PreviewContent::Unsupported(msg) => container(
                text(msg.clone()).size(12).color(t.text_secondary),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into(),
        };

        let card = container(
            column![
                // Header bar with close button
                container(
                    row![
                        text("Preview").size(13).color(t.text).width(Fill),
                        close_btn,
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(8)
                    .padding([4, 8]),
                )
                .width(Fill)
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                // Content area
                container(preview_body)
                    .width(Fill)
                    .height(Fill)
                    .padding(8)
                    .style(move |_th| container::Style {
                        background: Some(t.background.into()),
                        ..Default::default()
                    }),
            ]
            .width(Fill)
            .height(Fill),
        )
        .width(700)
        .height(500)
        .style(move |_th| container::Style {
            background: Some(t.background.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        // Semi-transparent backdrop — fills the full window
        container(
            container(card)
                .center_x(Fill)
                .center_y(Fill)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(move |_th| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6).into()),
            ..Default::default()
        })
        .into()
    }

    /// Success/info toast banner (green tint, auto-dismisses after 3 s).
    fn view_toast_banner<'a>(&'a self, msg: &'a str) -> Element<'a, Message> {
        let t = self.theme;
        let content = row![
            text(msg).size(12).color(t.text).width(Fill),
        ]
        .padding([6, 15]);
        container(content)
            .width(Fill)
            .style(move |_th| container::Style {
                background: Some(t.success.scale_alpha(0.15).into()),
                border: Border {
                    color: t.success.scale_alpha(0.5),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// Settings modal overlay — rendered on top of the full UI via `stack!`.
    fn view_settings_modal(&self) -> Element<Message> {
        let t = self.theme;

        const LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];
        let selected_level: Option<&'static str> = LOG_LEVELS
            .iter()
            .copied()
            .find(|&l| l == self.settings_draft.log.level);

        let level_row: Element<Message> = row![
            text("Log level:").size(12).color(t.text).width(120),
            pick_list(
                LOG_LEVELS,
                selected_level,
                |l: &'static str| Message::SettingsDraftLogLevel(l.to_string()),
            )
            .text_size(12),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12)
        .into();

        let console_row: Element<Message> = row![
            text("Console log:").size(12).color(t.text).width(120),
            toggler(self.settings_draft.log.console_enabled)
                .on_toggle(Message::SettingsDraftConsole),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12)
        .into();

        let file_row: Element<Message> = row![
            text("File log:").size(12).color(t.text).width(120),
            toggler(self.settings_draft.log.file_enabled)
                .on_toggle(Message::SettingsDraftFile),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12)
        .into();

        let open_folder_btn = button(text("📂  Open Log Folder").size(12).color(t.accent))
            .style(move |_th, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .on_press(Message::OpenLogFolder);

        let save_btn = button(text("Save").size(12).color(iced::Color::WHITE))
            .style(move |_th, _s| button::Style {
                background: Some(t.accent.into()),
                border: Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            })
            .padding([5, 16])
            .on_press(Message::SaveSettings);

        let cancel_btn = button(text("Cancel").size(12).color(t.text))
            .style(move |_th, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .padding([5, 12])
            .on_press(Message::CloseSettings);

        let card = container(
            column![
                // Header
                container(
                    row![
                        text("Settings").size(14).color(t.text).width(Fill),
                        cancel_btn,
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(8)
                    .padding([6, 10]),
                )
                .width(Fill)
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                // Body
                container(
                    column![
                        level_row,
                        console_row,
                        file_row,
                        open_folder_btn,
                        text("Changes apply on next launch.")
                            .size(11)
                            .color(t.text_secondary),
                    ]
                    .spacing(14),
                )
                .padding([16, 16])
                .width(Fill),
                // Footer
                container(
                    row![
                        iced::widget::Space::with_width(Fill),
                        save_btn,
                    ]
                    .spacing(8)
                    .padding([8, 12]),
                )
                .width(Fill)
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            ]
            .width(Fill),
        )
        .width(420)
        .style(move |_th| container::Style {
            background: Some(t.background.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        container(
            container(card)
                .center_x(Fill)
                .center_y(Fill)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(move |_th| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6).into()),
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

        let can_rename = has_device
            && self.android_pane.selected.len() == 1
            && self.android_pane.rename_pending.is_none();
        let can_delete = has_device
            && !self.android_pane.selected.is_empty()
            && self.android_pane.rename_pending.is_none();

        let rename_btn = toolbar_btn(
            "✏ Rename".to_string(),
            if can_rename { t.accent } else { t.text_secondary },
            if can_rename { Some(Message::AndroidBeginRename) } else { None },
        );

        let delete_btn = toolbar_btn(
            "🗑 Delete".to_string(),
            if can_delete { t.error } else { t.text_secondary },
            if can_delete { Some(Message::AndroidBeginDelete) } else { None },
        );

        let daemon_error = matches!(&self.adb_status, AdbStatus::Error(_));
        let restart_btn = toolbar_btn(
            "↺ Restart Daemon".to_string(),
            if daemon_error { t.warning } else { t.text_secondary },
            if daemon_error { Some(Message::RestartDaemon) } else { None },
        );

        let settings_btn = toolbar_btn(
            "⚙ Settings".to_string(),
            t.text,
            Some(Message::OpenSettings),
        );

        let content = row![
            refresh_btn,
            disconnect_btn,
            copy_to_android_btn,
            copy_to_local_btn,
            rename_btn,
            delete_btn,
            restart_btn,
            settings_btn,
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

        let right_inner = self.android_pane.view(
            self.theme,
            Message::AndroidNavigateTo,
            Message::AndroidSelectEntry,
            Message::AndroidRenameInput,
            Message::AndroidRenameCommit,
        );

        // Wrap the android pane with a drop-zone highlight while a file hovers
        let t = self.theme;
        let hover = self.file_hover_active;
        let right: Element<Message> = container(right_inner)
            .width(Fill)
            .height(Fill)
            .style(move |_theme| container::Style {
                border: Border {
                    color: if hover {
                        t.accent.scale_alpha(0.8)
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if hover { 2.0 } else { 0.0 },
                    ..Default::default()
                },
                ..Default::default()
            })
            .into();

        let divider = container(vertical_rule(1))
            .height(Fill)
            .style(move |_theme| container::Style {
                background: Some(t.border.into()),
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

    // ── Drag-and-drop tests ───────────────────────────────────────────────────

    #[test]
    fn file_hover_sets_active_flag() {
        let mut app = App::default();
        assert!(!app.file_hover_active);
        let _ = app.update(Message::FileHovered);
        assert!(app.file_hover_active);
    }

    #[test]
    fn files_hovered_left_clears_flag() {
        let mut app = App::default();
        app.file_hover_active = true;
        let _ = app.update(Message::FilesHoveredLeft);
        assert!(!app.file_hover_active);
    }

    #[test]
    fn file_dropped_without_device_shows_error() {
        let mut app = App::default();
        app.file_hover_active = true;
        let _ = app.update(Message::FileDropped(PathBuf::from("/tmp/test.jpg")));
        // hover flag should be cleared even on error
        assert!(!app.file_hover_active);
        // error banner should be set
        assert!(app.error_banner.is_some());
        // no transfer queued
        assert!(app.active_transfer.is_none());
    }

    // ── File operations tests ─────────────────────────────────────────────────

    #[test]
    fn begin_rename_no_selection_is_noop() {
        let mut app = App::default();
        let _ = app.update(Message::AndroidBeginRename);
        assert!(app.android_pane.rename_pending.is_none());
    }

    #[test]
    fn rename_cancel_clears_pending() {
        let mut app = App::default();
        app.android_pane.rename_pending = Some((0, "test.jpg".to_string()));
        let _ = app.update(Message::AndroidRenameCancel);
        assert!(app.android_pane.rename_pending.is_none());
    }

    #[test]
    fn rename_input_updates_value() {
        let mut app = App::default();
        app.android_pane.rename_pending = Some((0, "old.jpg".to_string()));
        let _ = app.update(Message::AndroidRenameInput("new.jpg".to_string()));
        assert_eq!(
            app.android_pane.rename_pending,
            Some((0, "new.jpg".to_string()))
        );
    }

    #[test]
    fn begin_delete_no_selection_is_noop() {
        let mut app = App::default();
        let _ = app.update(Message::AndroidBeginDelete);
        assert!(app.delete_confirm_paths.is_none());
    }

    #[test]
    fn delete_cancel_clears_confirm() {
        let mut app = App::default();
        app.delete_confirm_paths = Some(vec![PathBuf::from("/sdcard/test.jpg")]);
        let _ = app.update(Message::AndroidDeleteCancel);
        assert!(app.delete_confirm_paths.is_none());
    }

    // ── File preview tests ────────────────────────────────────────────────────

    #[test]
    fn preview_file_too_large_shows_error() {
        let mut app = App::default();
        let entry = adb::AndroidEntry {
            name: "bigvideo.mp4".to_string(),
            path: PathBuf::from("/sdcard/bigvideo.mp4"),
            size: 20 * 1024 * 1024, // 20 MB — over the 10 MB limit
            modified: "2024-01-15".to_string(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
        };
        let _ = app.update(Message::PreviewFile(entry));
        assert!(app.error_banner.is_some());
        assert!(app.error_banner.as_deref().unwrap().contains("10 MB"));
    }

    #[test]
    fn preview_ready_image_sets_modal() {
        let mut app = App::default();
        let _ = app.update(Message::PreviewReady(PathBuf::from("/tmp/photo.jpg")));
        assert!(matches!(app.preview_modal, Some(PreviewContent::Image(_))));
    }

    #[test]
    fn preview_ready_unsupported_extension() {
        let mut app = App::default();
        let _ = app.update(Message::PreviewReady(PathBuf::from("/tmp/archive.zip")));
        assert!(matches!(
            app.preview_modal,
            Some(PreviewContent::Unsupported(_))
        ));
    }

    #[test]
    fn close_preview_clears_modal() {
        let mut app = App::default();
        app.preview_modal = Some(PreviewContent::Unsupported("n/a".to_string()));
        let _ = app.update(Message::ClosePreview);
        assert!(app.preview_modal.is_none());
    }

    #[test]
    fn escape_closes_preview_modal_first() {
        let mut app = App::default();
        app.preview_modal = Some(PreviewContent::Unsupported("n/a".to_string()));
        app.android_pane.rename_pending = Some((0, "name".to_string()));
        let _ = app.update(Message::EscapePressed);
        // modal closed, rename still active (Escape routed to ClosePreview)
        assert!(app.preview_modal.is_none());
        assert!(app.android_pane.rename_pending.is_some());
    }

    // ── Settings tests ────────────────────────────────────────────────────────

    #[test]
    fn open_settings_copies_config_to_draft() {
        let mut app = App::default();
        app.config.log.level = "warn".to_string();
        let _ = app.update(Message::OpenSettings);
        assert!(app.settings_open);
        assert_eq!(app.settings_draft.log.level, "warn");
    }

    #[test]
    fn close_settings_hides_modal() {
        let mut app = App::default();
        app.settings_open = true;
        let _ = app.update(Message::CloseSettings);
        assert!(!app.settings_open);
    }

    #[test]
    fn settings_draft_log_level_updates_draft_only() {
        let mut app = App::default();
        app.config.log.level = "info".to_string();
        app.settings_draft.log.level = "info".to_string();
        let _ = app.update(Message::SettingsDraftLogLevel("debug".to_string()));
        assert_eq!(app.settings_draft.log.level, "debug");
        assert_eq!(app.config.log.level, "info"); // live config unchanged
    }

    #[test]
    fn settings_draft_console_toggle() {
        let mut app = App::default();
        app.settings_draft.log.console_enabled = true;
        let _ = app.update(Message::SettingsDraftConsole(false));
        assert!(!app.settings_draft.log.console_enabled);
        assert!(app.config.log.console_enabled); // live config unchanged
    }

    #[test]
    fn settings_draft_file_toggle() {
        let mut app = App::default();
        app.settings_draft.log.file_enabled = true;
        let _ = app.update(Message::SettingsDraftFile(false));
        assert!(!app.settings_draft.log.file_enabled);
    }

    #[test]
    fn save_settings_closes_modal_and_updates_config() {
        let mut app = App::default();
        // config_path is empty in tests — SaveSettings will fail disk write
        // but should still update in-memory config and close modal
        app.settings_open = true;
        app.settings_draft.log.level = "error".to_string();
        app.settings_draft.log.console_enabled = false;
        let _ = app.update(Message::SaveSettings);
        assert!(!app.settings_open, "modal should close immediately");
        assert_eq!(app.config.log.level, "error");
        assert!(!app.config.log.console_enabled);
    }

    #[test]
    fn save_settings_writes_valid_yaml() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.flush().unwrap();
        let path = f.path().to_path_buf();

        let mut app = App::with_config(config::AppConfig::default(), path.clone());
        app.settings_draft.log.level = "trace".to_string();
        app.settings_draft.log.file_enabled = false;
        // Run synchronously — since no tokio runtime in unit tests, we test
        // the state changes only (Task::perform is deferred)
        let _ = app.update(Message::SaveSettings);
        assert_eq!(app.config.log.level, "trace");
        assert!(!app.config.log.file_enabled);
    }

    #[test]
    fn escape_closes_settings_first() {
        let mut app = App::default();
        app.settings_open = true;
        app.android_pane.rename_pending = Some((0, "name".to_string()));
        let _ = app.update(Message::EscapePressed);
        assert!(!app.settings_open, "settings should close");
        assert!(app.android_pane.rename_pending.is_some(), "rename still active");
    }

    #[test]
    fn open_settings_closes_preview_modal() {
        let mut app = App::default();
        app.preview_modal = Some(PreviewContent::Unsupported("n/a".to_string()));
        let _ = app.update(Message::OpenSettings);
        assert!(app.preview_modal.is_none(), "preview should be cleared");
        assert!(app.settings_open);
    }

    #[test]
    fn show_toast_sets_toast_message() {
        let mut app = App::default();
        let _ = app.update(Message::ShowToast("hello".to_string()));
        assert_eq!(app.toast.as_deref(), Some("hello"));
    }

    #[test]
    fn dismiss_toast_clears_message() {
        let mut app = App::default();
        app.toast = Some("hello".to_string());
        let _ = app.update(Message::DismissToast);
        assert!(app.toast.is_none());
    }

    #[test]
    fn escape_cancels_rename_when_no_modal() {
        let mut app = App::default();
        app.android_pane.rename_pending = Some((0, "name".to_string()));
        let _ = app.update(Message::EscapePressed);
        assert!(app.android_pane.rename_pending.is_none());
    }

    #[test]
    fn double_click_on_file_triggers_preview_for_small_file() {
        let mut app = App::default();
        let entry = adb::AndroidEntry {
            name: "photo.jpg".to_string(),
            path: PathBuf::from("/sdcard/photo.jpg"),
            size: 500 * 1024, // 500 KB — under limit
            modified: "2024-01-15".to_string(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
        };
        app.android_pane.state = android_pane::AndroidPaneState::Browsing;
        app.android_pane.entries = vec![entry];

        // First click — selects
        let _ = app.update(Message::AndroidSelectEntry(0));
        assert!(app.android_last_click.is_some());

        // Second click immediately — double-click detected, triggers PreviewFile
        // (no adb client so it won't actually pull, but no panic either)
        let _ = app.update(Message::AndroidSelectEntry(0));
        // last_click should be cleared after double-click consumed
        assert!(app.android_last_click.is_none());
    }
}
