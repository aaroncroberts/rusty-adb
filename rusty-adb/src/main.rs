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
mod config;
mod file_pane;
mod fs;
mod icons;
mod queue;
#[cfg(test)]
mod tests;
mod theme;
mod update;
mod views;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use adb::{AdbClient, AdbDevice};
use adb::{AdbStatus, TransferEvent, TransferJob, TransferStatus};
use file_pane::FilePane;
use fs::{AndroidContext, AndroidFs, DirEntry, FileSystem, LocalFs, PaneState, SortField};
use queue::QueueManager;
use theme::ThemeColors;
use views::status_bar::StatusBar;

use iced::keyboard::{self, key::Named};
use iced::{Subscription, Task, Theme};

// ─── View Mode ─────────────────────────────────────────────────────────────────

/// Rendering style for directory entries in each pane.
///
/// Each pane (local and android) holds its own `ViewMode` independently so
/// the user can, for example, view local files in Details mode while browsing
/// the android device in List mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ViewMode {
    /// Single-column filename rows — the default, highest-density view.
    #[default]
    List,
    /// Tabular view with Name, Size, Modified, and Type columns.
    Details,
    /// Multi-column compact tiles (4 per row) with type icon and filename.
    Grid,
    /// Large tiles (2–3 per row) with a big type icon and filename below.
    Icon,
}

impl ViewMode {
    /// All variants in the order shown in the drop-down picker.
    #[allow(dead_code)]
    const ALL: &'static [ViewMode] = &[
        ViewMode::Icon,
        ViewMode::List,
        ViewMode::Grid,
        ViewMode::Details,
    ];
}

impl std::fmt::Display for ViewMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewMode::Icon => f.write_str("As Icons"),
            ViewMode::List => f.write_str("As List"),
            ViewMode::Grid => f.write_str("As Gallery"),
            ViewMode::Details => f.write_str("As Columns"),
        }
    }
}

/// Layout state for the two file panes.
///
/// In `Split` mode both panes share equal width (the default).  In either
/// expanded mode one pane fills the main area while the other collapses to a
/// narrow sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PaneLayout {
    /// Equal-width split — the default view.
    #[default]
    Split,
    /// Local pane is expanded; Android pane becomes a narrow sidebar on the right.
    LocalExpanded,
    /// Android pane is expanded; local pane becomes a narrow sidebar on the left.
    AndroidExpanded,
}

/// Minimum log level shown in the in-app log viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    const ALL: &'static [LogLevel] = &[
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ];

    /// Returns `true` if a raw log line's level is at or above `self`.
    fn matches(self, line: &str) -> bool {
        let line_upper = line.to_uppercase();
        // Detect the level of this line, then check if it meets the threshold
        let line_level = if line_upper.contains(" ERROR") || line_upper.contains("[ERROR]") {
            LogLevel::Error
        } else if line_upper.contains(" WARN") || line_upper.contains("[WARN]") {
            LogLevel::Warn
        } else if line_upper.contains(" INFO") || line_upper.contains("[INFO]") {
            LogLevel::Info
        } else if line_upper.contains(" DEBUG") || line_upper.contains("[DEBUG]") {
            LogLevel::Debug
        } else if line_upper.contains(" TRACE") || line_upper.contains("[TRACE]") {
            LogLevel::Trace
        } else {
            // Continuation / unrecognised lines — include unless we're at Error level only
            return self != LogLevel::Error;
        };
        line_level as u8 >= self as u8
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => f.write_str("TRACE+"),
            LogLevel::Debug => f.write_str("DEBUG+"),
            LogLevel::Info => f.write_str("INFO+"),
            LogLevel::Warn => f.write_str("WARN+"),
            LogLevel::Error => f.write_str("ERROR"),
        }
    }
}

// ─── Device Details Tab ───────────────────────────────────────────────────────

/// Which tab is active in the device details modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DeviceTab {
    #[default]
    Device,
    OsBuild,
    Connection,
    Display,
    Apps,
}

impl std::fmt::Display for DeviceTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceTab::Device => f.write_str("Device"),
            DeviceTab::OsBuild => f.write_str("OS / Build"),
            DeviceTab::Connection => f.write_str("Connection"),
            DeviceTab::Display => f.write_str("Display"),
            DeviceTab::Apps => f.write_str("Apps"),
        }
    }
}

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    // ── ADB ──────────────────────────────────────────────────────────────────
    AdbReady(Arc<AdbClient>),
    PollDevices,
    DevicesLoaded(Arc<Vec<AdbDevice>>),
    AdbError(String),

    // ── Pane Layout ───────────────────────────────────────────────────────────
    /// Expand one pane: `true` = android, `false` = local.
    ExpandPane(bool),
    /// Restore the equal-split layout from any expanded state.
    CollapsePanes,

    // ── View Modes ────────────────────────────────────────────────────────────
    /// Switch the local pane to the given view mode
    SetLocalViewMode(ViewMode),
    /// Switch the android pane to the given view mode
    SetAndroidViewMode(ViewMode),
    /// Select the item shown in the large Gallery preview (local pane)
    LocalGallerySelect(usize),
    /// Select the item shown in the large Gallery preview (android pane)
    AndroidGallerySelect(usize),

    // ── Local Pane ────────────────────────────────────────────────────────────
    LocalNavigateTo(PathBuf),
    LocalEntriesLoaded {
        path: PathBuf,
        entries: Vec<DirEntry>,
    },
    LocalLoadError(String),
    LocalSelectEntry(usize),
    ToggleShowPanel(bool),
    LocalToggleHidden,
    LocalToggleType,
    LocalToggleSize,
    LocalToggleModified,
    LocalSortBy(SortField),

    // ── Android Pane ──────────────────────────────────────────────────────────
    AndroidToggleHidden,
    AndroidToggleType,
    AndroidToggleSize,
    AndroidToggleModified,
    AndroidNavigateTo(PathBuf),
    AndroidEntriesLoaded {
        path: PathBuf,
        entries: Vec<DirEntry>,
        roots: Vec<PathBuf>,
    },
    AndroidLoadError(String),
    AndroidSelectEntry(usize),
    AndroidSortBy(SortField),
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
    /// User pressed "Copy →" (local → android) — still used by transfer tests
    #[allow(dead_code)]
    CopyToAndroid,
    /// User pressed "Copy ←" (android → local)
    CopyToLocal,
    /// Progress event from the active transfer subscription
    TransferProgress {
        percent: u8,
    },
    /// Transfer completed — carries final speed for display
    TransferComplete {
        speed_display: String,
    },
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
    /// User pressed mouse button on the local pane while files are selected — starts in-app drag
    LocalDragStarted,
    /// User released the mouse over the android pane while an in-app drag was in progress
    DroppedOnAndroid,
    /// In-app drag was cancelled (Escape or mouse released elsewhere)
    DragCancelled,

    // ── Copy Queue ────────────────────────────────────────────────────────────
    /// User clicked "Cancel" or Escape in the copy confirm dialog
    CloseCopyConfirm,
    /// User clicked "Confirm" in the copy confirm dialog → enqueue items
    ConfirmCopyToDevice,
    /// Open the queue management dialog
    OpenQueueDialog,
    /// Close the queue management dialog
    CloseQueueDialog,
    /// Pause/resume the copy queue
    ToggleQueuePause,
    /// Remove an item from the queue by id
    QueueRemoveItem(u64),
    /// Begin editing the destination of a queue item (id)
    QueueEditItem(u64),
    /// User typed a new destination path for the item being edited
    QueueEditDestInput(String),
    /// Confirm the new destination for the item being edited
    QueueEditDestConfirm,
    /// A queued copy completed (fired by the copy engine)
    QueueItemComplete(u64),
    /// A queued copy failed (fired by the copy engine)
    QueueItemFailed { id: u64, reason: String },

    // ── Device Details dialog ─────────────────────────────────────────────────
    /// Open the device details modal and start fetching properties
    OpenDeviceDetails,
    /// Close the device details modal
    CloseDeviceDetails,
    /// ADB property fetch completed — store and show the result
    DeviceDetailsLoaded(crate::adb::DeviceDetails),
    /// ADB property fetch failed
    DeviceDetailsFailed(String),
    /// User clicked Refresh — re-fetch all properties
    RefreshDeviceDetails,
    /// User clicked a tab in the device details modal
    DeviceDetailsSelectTab(DeviceTab),

    // ── About dialog ──────────────────────────────────────────────────────────
    /// Open the About modal
    OpenAbout,
    /// Close the About modal
    CloseAbout,

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
    /// Open the in-app log viewer
    OpenLogViewer,
    /// Close the in-app log viewer
    CloseLogViewer,
    /// Populate log viewer with the list of log files found in the app dir
    LogViewerFilesLoaded(Vec<std::path::PathBuf>),
    /// User picked a different log file to view
    LogViewerSelectFile(std::path::PathBuf),
    /// Raw content of the selected log file loaded from disk
    LogViewerFileLoaded(String),
    /// User changed the level filter in the log viewer
    LogViewerSetLevel(LogLevel),
    /// Show a transient success/info toast (auto-dismisses after 3 s)
    ShowToast(String),
    /// Dismiss the info toast
    DismissToast,

    // ── File Preview ──────────────────────────────────────────────────────────
    /// Double-click on a file entry — check size then pull to temp
    PreviewFile(DirEntry),
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

    // ── App Management ────────────────────────────────────────────────────────
    /// Package list loaded from the device
    AppsLoaded(Vec<crate::adb::InstalledApp>),
    /// Package list fetch failed
    AppsFailed(String),
    /// User selected a package in the apps list
    AppsSelectPackage(String),
    /// User pressed Uninstall — sets confirm flag (does NOT yet run ADB)
    UninstallApp,
    /// User confirmed the uninstall dialog — runs `adb uninstall <package_id>`
    UninstallConfirmed,
    /// User cancelled the uninstall confirmation
    UninstallCancel,
    /// `adb uninstall` succeeded
    UninstallComplete,
    /// `adb uninstall` failed
    UninstallFailed(String),
    /// User pressed "Install APK" toolbar button — captures path for confirm banner
    InstallApk,
    /// User confirmed the install APK banner — runs `adb install -r <path>`
    InstallApkConfirmed,
    /// User cancelled the install APK confirmation banner
    InstallApkCancel,
    /// `adb install` succeeded — carries the APK filename
    InstallApkComplete(String),
    /// `adb install` failed
    InstallApkFailed(String),
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

// ─── App Icon ─────────────────────────────────────────────────────────────────

/// Build a simple placeholder 32×32 icon from inline RGBA data.
///
/// Returns `None` if icon creation fails (non-fatal — app still launches).
/// Replace this with a real `.icns`/`.ico` at packaging time.
fn placeholder_icon() -> Option<iced::window::Icon> {
    const SIZE: u32 = 32;
    // Teal (#1a9e8a) squares with a dark border — minimal but recognisable
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for row in 0..SIZE {
        for col in 0..SIZE {
            let is_border = row == 0 || row == SIZE - 1 || col == 0 || col == SIZE - 1;
            let (r, g, b, a) = if is_border {
                (0x0d, 0x6e, 0x60, 0xff) // dark teal border
            } else {
                (0x1a, 0x9e, 0x8a, 0xff) // teal fill
            };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    iced::window::icon::from_rgba(rgba, SIZE, SIZE).ok()
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

    local_pane: FilePane<LocalFs>,
    android_pane: FilePane<AndroidFs>,
    android_ctx: Option<AndroidContext>,

    /// Active view mode for the local pane (persists during navigation)
    local_view_mode: ViewMode,
    /// Active view mode for the android pane (persists during navigation)
    android_view_mode: ViewMode,
    /// Current pane layout: equal split, local expanded, or android expanded.
    pane_layout: PaneLayout,

    /// Index of the item shown in the large preview area (Gallery view)
    local_gallery_idx: usize,
    /// Index of the item shown in the large preview area (Gallery view)
    android_gallery_idx: usize,

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
    /// `true` while an in-app drag from the local pane is in progress
    drag_in_progress: bool,
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
    /// Whether the About modal is currently open
    about_open: bool,
    /// Which pane's column/visibility panel is open: Some(false)=local, Some(true)=android, None=closed
    show_panel_open: Option<bool>,

    // ── Device Details dialog ─────────────────────────────────────────────────
    /// Whether the device details modal is open
    device_details_open: bool,
    /// Fetched device details (None = not yet loaded)
    device_details: Option<crate::adb::DeviceDetails>,
    /// True while the async fetch is in flight
    device_details_loading: bool,
    /// Which tab is currently active in the device details modal
    device_details_tab: DeviceTab,

    // ── Copy Queue ────────────────────────────────────────────────────────────
    /// Persistent background copy queue (local → Android)
    copy_queue: QueueManager,
    /// Whether the queue management dialog is open
    queue_open: bool,
    /// Whether the copy-to-device confirmation dialog is open
    copy_confirm_open: bool,
    /// Item being edited in the queue dialog: (item_id, current_dest_input)
    queue_editing: Option<(u64, String)>,

    // ── App Management ────────────────────────────────────────────────────────
    /// User-installed packages loaded from the device (None = not yet loaded)
    installed_apps: Option<Vec<crate::adb::InstalledApp>>,
    /// True while `pm list packages` is in flight
    apps_loading: bool,
    /// Package ID of the currently selected app in the list
    apps_selected: Option<String>,
    /// True when Uninstall was clicked — waits for user confirmation
    uninstall_confirm: bool,
    /// True while `adb uninstall` is in flight
    apps_uninstalling: bool,
    /// APK path pending install confirmation — Some(path) shows the confirm banner
    install_apk_confirm: Option<PathBuf>,
    /// True while `adb install` is in flight
    apps_installing: bool,

    // ── Log viewer ────────────────────────────────────────────────────────────
    /// Whether the log viewer modal is open
    log_viewer_open: bool,
    /// Log files found in the app data directory
    log_viewer_files: Vec<std::path::PathBuf>,
    /// Currently selected log file path
    log_viewer_selected: Option<std::path::PathBuf>,
    /// Raw content of the currently selected log file
    log_viewer_content: String,
    /// Pre-filtered display string — recomputed only when content or level changes
    log_viewer_display: String,
    /// Minimum level filter applied to displayed lines
    log_viewer_level: LogLevel,
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
            local_pane: FilePane::new(start_path, PaneState::Loading),
            android_pane: FilePane::new(PathBuf::from("/sdcard"), PaneState::NoDevice),
            android_ctx: None,
            local_view_mode: ViewMode::default(),
            android_view_mode: ViewMode::default(),
            pane_layout: PaneLayout::default(),
            local_gallery_idx: 0,
            android_gallery_idx: 0,
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
            drag_in_progress: false,
            delete_confirm_paths: None,
            preview_modal: None,
            android_last_click: None,
            installed_apps: None,
            apps_loading: false,
            apps_selected: None,
            uninstall_confirm: false,
            apps_uninstalling: false,
            install_apk_confirm: None,
            apps_installing: false,
            log_viewer_open: false,
            log_viewer_files: Vec::new(),
            log_viewer_selected: None,
            log_viewer_content: String::new(),
            log_viewer_display: String::new(),
            log_viewer_level: LogLevel::Info,
            copy_queue: {
                let path = dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".rusty-adb")
                    .join("queue.json");
                QueueManager::load(path)
            },
            device_details_open: false,
            device_details: None,
            device_details_loading: false,
            device_details_tab: DeviceTab::default(),
            queue_open: false,
            copy_confirm_open: false,
            queue_editing: None,
            config: config::AppConfig::default(),
            config_path: PathBuf::new(),
            settings_open: false,
            settings_draft: config::AppConfig::default(),
            toast: None,
            about_open: false,
            show_panel_open: None,
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
        |app: &App| {
            let version = env!("CARGO_PKG_VERSION");
            match &app.adb_status {
                AdbStatus::Connected(name) => {
                    format!("rusty-adb v{version} - Android File Manager - {name}")
                }
                _ => format!("rusty-adb v{version} - Android File Manager"),
            }
        },
        App::update,
        App::view,
    )
    .subscription(App::subscription)
    .font(icons::FONT_BYTES)
    .theme(|_| Theme::TokyoNightStorm)
    .window(iced::window::Settings {
        size: iced::Size::new(1280.0, 800.0),
        min_size: Some(iced::Size::new(900.0, 600.0)),
        icon: placeholder_icon(),
        ..Default::default()
    })
    .run_with(|| {
        // Kick off ADB: kill any stale daemon, then start fresh.
        let adb_task = Task::perform(
            async {
                let client = AdbClient::find().await.map_err(|e| e.to_string())?;
                // start-server is a no-op if already running; avoids disrupting
                // devices that are already authorized and connected.
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

        // Load the local pane immediately — doesn't need ADB to be ready.
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let load_path = home_dir.clone();
        let local_task = Task::perform(
            async move {
                LocalFs::list_dir(&(), &load_path)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| match result {
                Ok(entries) => Message::LocalEntriesLoaded {
                    path: home_dir.clone(),
                    entries,
                },
                Err(e) => Message::LocalLoadError(e),
            },
        );

        (
            App::with_config(cfg, config_path),
            Task::batch([adb_task, local_task]),
        )
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
        let device_poll = iced::time::every(Duration::from_secs(2)).map(|_| Message::PollDevices);

        // Spinner — only while the Android pane is loading
        let maybe_spinner = if self.android_pane.state == PaneState::Loading {
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
                    let result = adb::run_transfer(&job, cancel, |event| {
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
                        if let Err(e) = sender.try_send(msg) {
                            tracing::warn!(error = ?e, "transfer event channel full — progress tick dropped");
                        }
                    })
                    .await;

                    if let Err(e) = result {
                        let msg = e.to_string();
                        tracing::warn!(error = %msg, "transfer engine error");
                        if let Err(send_err) = sender.try_send(Message::TransferFailed(msg)) {
                            tracing::error!(error = ?send_err, "failed to deliver TransferFailed to UI — user will not see error");
                        }
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
        let file_drops = iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Window(iced::window::Event::FileHovered(_)) => Some(Message::FileHovered),
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
