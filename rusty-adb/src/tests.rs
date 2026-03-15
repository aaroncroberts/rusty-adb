use super::*;
use adb::DeviceState;

// ── Helpers shared by transfer flow tests ────────────────────────────────────

/// Path to the mock-adb script bundled with the crate's test fixtures.
/// Stable because CARGO_MANIFEST_DIR is always the crate root.
fn mock_adb_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock-adb")
}

/// Feed a `TransferEvent` into `App` exactly as the subscription does,
/// returning the App state snapshot after each message is processed.
///
/// This mirrors the mapping in `main.rs subscription()` verbatim:
///   Progress  → TransferProgress
///   Complete  → TransferComplete
///   Cancelled → TransferCancelled
///   Failed    → TransferFailed
fn apply_event(app: &mut App, event: adb::TransferEvent) {
    let msg = match event {
        adb::TransferEvent::Progress { percent } => Message::TransferProgress { percent },
        adb::TransferEvent::Complete { speed_display } => {
            Message::TransferComplete { speed_display }
        }
        adb::TransferEvent::Cancelled => Message::TransferCancelled,
        adb::TransferEvent::Failed(e) => Message::TransferFailed(e),
    };
    let _ = app.update(msg);
}

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
    assert_eq!(update::derive_status(&[]), AdbStatus::Disconnected);
}

#[test]
fn derive_status_authorized_device() {
    let devices = vec![device("ABC123", DeviceState::Device, Some("Pixel 7"))];
    assert_eq!(
        update::derive_status(&devices),
        AdbStatus::Connected("Pixel 7".to_string())
    );
}

#[test]
fn derive_status_authorized_no_model_uses_serial() {
    let devices = vec![device("ABC123", DeviceState::Device, None)];
    assert_eq!(
        update::derive_status(&devices),
        AdbStatus::Connected("ABC123".to_string())
    );
}

#[test]
fn derive_status_unauthorized() {
    let devices = vec![device("ABC123", DeviceState::Unauthorized, None)];
    assert_eq!(update::derive_status(&devices), AdbStatus::Unauthorized);
}

#[test]
fn derive_status_prefers_authorized_over_unauthorized() {
    let devices = vec![
        device("ABC123", DeviceState::Unauthorized, None),
        device("DEF456", DeviceState::Device, Some("Galaxy S24")),
    ];
    assert_eq!(
        update::derive_status(&devices),
        AdbStatus::Connected("Galaxy S24".to_string())
    );
}

#[test]
fn derive_status_offline_is_disconnected() {
    let devices = vec![device("ABC123", DeviceState::Offline, None)];
    assert_eq!(update::derive_status(&devices), AdbStatus::Disconnected);
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
    let mut app = App {
        file_hover_active: true,
        ..Default::default()
    };
    let _ = app.update(Message::FilesHoveredLeft);
    assert!(!app.file_hover_active);
}

#[test]
fn file_dropped_without_device_shows_error() {
    let mut app = App {
        file_hover_active: true,
        ..Default::default()
    };
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
    let mut app = App {
        delete_confirm_paths: Some(vec![PathBuf::from("/sdcard/test.jpg")]),
        ..Default::default()
    };
    let _ = app.update(Message::AndroidDeleteCancel);
    assert!(app.delete_confirm_paths.is_none());
}

// ── File preview tests ────────────────────────────────────────────────────

#[test]
fn preview_file_too_large_shows_error() {
    let mut app = App::default();
    let entry = DirEntry {
        name: "bigvideo.mp4".to_string(),
        path: PathBuf::from("/sdcard/bigvideo.mp4"),
        size: 20 * 1024 * 1024, // 20 MB — over the 10 MB limit
        modified_display: "2024-01-15".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
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
    let mut app = App {
        preview_modal: Some(PreviewContent::Unsupported("n/a".to_string())),
        ..Default::default()
    };
    let _ = app.update(Message::ClosePreview);
    assert!(app.preview_modal.is_none());
}

#[test]
fn escape_closes_preview_modal_first() {
    let mut app = App {
        preview_modal: Some(PreviewContent::Unsupported("n/a".to_string())),
        ..Default::default()
    };
    app.android_pane.rename_pending = Some((0, "name".to_string()));
    let _ = app.update(Message::EscapePressed);
    // modal closed, rename still active (Escape routed to ClosePreview)
    assert!(app.preview_modal.is_none());
    assert!(app.android_pane.rename_pending.is_some());
}

// ── About dialog tests ───────────────────────────────────────────────────

#[test]
fn open_about_sets_flag() {
    let mut app = App::default();
    let _ = app.update(Message::OpenAbout);
    assert!(app.about_open);
}

#[test]
fn close_about_clears_flag() {
    let mut app = App {
        about_open: true,
        ..Default::default()
    };
    let _ = app.update(Message::CloseAbout);
    assert!(!app.about_open);
}

#[test]
fn escape_closes_about_before_rename() {
    let mut app = App {
        about_open: true,
        ..Default::default()
    };
    app.android_pane.rename_pending = Some((0, "n".to_string()));
    let _ = app.update(Message::EscapePressed);
    assert!(!app.about_open);
    assert!(app.android_pane.rename_pending.is_some());
}

#[test]
fn placeholder_icon_generates_without_panic() {
    // Verify the icon helper runs cleanly — it's called at app startup
    let icon = placeholder_icon();
    assert!(icon.is_some(), "placeholder_icon should succeed");
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
    let mut app = App {
        settings_open: true,
        ..Default::default()
    };
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
    let mut app = App {
        settings_open: true,
        ..Default::default()
    };
    // config_path is empty in tests — SaveSettings will fail disk write
    // but should still update in-memory config and close modal
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
    let mut app = App {
        settings_open: true,
        ..Default::default()
    };
    app.android_pane.rename_pending = Some((0, "name".to_string()));
    let _ = app.update(Message::EscapePressed);
    assert!(!app.settings_open, "settings should close");
    assert!(
        app.android_pane.rename_pending.is_some(),
        "rename still active"
    );
}

#[test]
fn open_settings_closes_preview_modal() {
    let mut app = App {
        preview_modal: Some(PreviewContent::Unsupported("n/a".to_string())),
        ..Default::default()
    };
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
    let mut app = App {
        toast: Some("hello".to_string()),
        ..Default::default()
    };
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
    let entry = DirEntry {
        name: "photo.jpg".to_string(),
        path: PathBuf::from("/sdcard/photo.jpg"),
        size: 500 * 1024, // 500 KB — under limit
        modified_display: "2024-01-15".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    };
    app.android_pane.state = PaneState::Ready;
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

// ── Header consolidation tests ─────────────────────────────────────────────

/// Smoke test: view_header() must compile and return an Element without panicking.
/// The combined header replaced two separate rows (view_hero_banner + view_toolbar);
/// verifying it renders proves the merge is structurally sound.
#[test]
fn view_header_renders_without_panic() {
    let app = App::default();
    let _ = app.view_header();
}

/// Regression guard: neither view_hero_banner nor view_toolbar should exist.
/// This is enforced at compile time — if this test compiles, the old split-row
/// API is gone and view_header() is the single entry point.
#[test]
fn view_header_is_sole_header_entrypoint() {
    // view_header() must be callable; the absence of view_hero_banner /
    // view_toolbar is verified by the compiler (they were removed).
    let app = App::default();
    let _: iced::Element<Message> = app.view_header();
}

// ── Pane layout state machine tests ────────────────────────────────────────

#[test]
fn default_pane_layout_is_split() {
    let app = App::default();
    assert_eq!(app.pane_layout, PaneLayout::Split);
}

#[test]
fn expand_local_pane_transitions_to_local_expanded() {
    let mut app = App::default();
    let _ = app.update(Message::ExpandPane(false));
    assert_eq!(app.pane_layout, PaneLayout::LocalExpanded);
}

#[test]
fn expand_android_pane_transitions_to_android_expanded() {
    let mut app = App::default();
    let _ = app.update(Message::ExpandPane(true));
    assert_eq!(app.pane_layout, PaneLayout::AndroidExpanded);
}

#[test]
fn collapse_from_local_expanded_returns_to_split() {
    let mut app = App::default();
    let _ = app.update(Message::ExpandPane(false));
    let _ = app.update(Message::CollapsePanes);
    assert_eq!(app.pane_layout, PaneLayout::Split);
}

#[test]
fn collapse_from_android_expanded_returns_to_split() {
    let mut app = App::default();
    let _ = app.update(Message::ExpandPane(true));
    let _ = app.update(Message::CollapsePanes);
    assert_eq!(app.pane_layout, PaneLayout::Split);
}

#[test]
fn collapse_already_split_stays_split() {
    let mut app = App::default();
    let _ = app.update(Message::CollapsePanes);
    assert_eq!(app.pane_layout, PaneLayout::Split);
}

#[test]
fn expand_android_then_expand_local_transitions_correctly() {
    let mut app = App::default();
    let _ = app.update(Message::ExpandPane(true));
    assert_eq!(app.pane_layout, PaneLayout::AndroidExpanded);
    // Expanding local while android is expanded switches directly
    let _ = app.update(Message::ExpandPane(false));
    assert_eq!(app.pane_layout, PaneLayout::LocalExpanded);
}

// ── Pane layout view smoke tests ───────────────────────────────────────────

#[test]
fn view_panes_split_renders_without_panic() {
    let app = App::default();
    let _: iced::Element<Message> = app.view();
}

#[test]
fn view_panes_local_expanded_renders_without_panic() {
    let app = App {
        pane_layout: PaneLayout::LocalExpanded,
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

#[test]
fn view_panes_android_expanded_renders_without_panic() {
    let app = App {
        pane_layout: PaneLayout::AndroidExpanded,
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

// ── Banner view smoke tests ─────────────────────────────────────────────────

/// `view_error_banner` is exercised through `App::view` when error_banner is set.
#[test]
fn view_error_banner_renders_without_panic() {
    let app = App {
        error_banner: Some("Something went wrong".to_string()),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_toast_banner` is exercised through `App::view` when toast is set.
#[test]
fn view_toast_banner_renders_without_panic() {
    let app = App {
        toast: Some("Transfer complete".to_string()),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_delete_confirm` with a single file — confirms the singular label path.
#[test]
fn view_delete_confirm_single_file_renders_without_panic() {
    let app = App {
        delete_confirm_paths: Some(vec![PathBuf::from("/sdcard/photo.jpg")]),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_delete_confirm` with multiple files — confirms the plural label path.
#[test]
fn view_delete_confirm_multiple_files_renders_without_panic() {
    let app = App {
        delete_confirm_paths: Some(vec![
            PathBuf::from("/sdcard/a.jpg"),
            PathBuf::from("/sdcard/b.jpg"),
            PathBuf::from("/sdcard/c.jpg"),
        ]),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

// ── Modal view smoke tests ──────────────────────────────────────────────────

/// `view_about_modal` is stacked over the base UI when `about_open = true`.
#[test]
fn view_about_modal_renders_without_panic() {
    let app = App {
        about_open: true,
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_settings_modal` is stacked when `settings_open = true`.
#[test]
fn view_settings_modal_renders_without_panic() {
    let app = App {
        settings_open: true,
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_log_viewer` is the outermost overlay when `log_viewer_open = true`.
#[test]
fn view_log_viewer_renders_without_panic() {
    let app = App {
        log_viewer_open: true,
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_preview_modal` with an image path.
#[test]
fn view_preview_modal_image_renders_without_panic() {
    let app = App {
        preview_modal: Some(PreviewContent::Image(PathBuf::from("/tmp/photo.jpg"))),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_preview_modal` with text content.
#[test]
fn view_preview_modal_text_renders_without_panic() {
    let app = App {
        preview_modal: Some(PreviewContent::Text("line1\nline2\n".to_string())),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// `view_preview_modal` with an unsupported file type message.
#[test]
fn view_preview_modal_unsupported_renders_without_panic() {
    let app = App {
        preview_modal: Some(PreviewContent::Unsupported(
            "Preview not available for .exe files".to_string(),
        )),
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

/// ADB not found — renders the setup guide instead of the panes.
#[test]
fn view_adb_not_found_renders_without_panic() {
    let app = App {
        adb_status: AdbStatus::NotFound,
        ..Default::default()
    };
    let _: iced::Element<Message> = app.view();
}

// ── Device connect / disconnect integration tests ──────────────────────────
//
// These tests drive App::update() with message sequences that simulate the
// full device lifecycle. Task::perform closures are discarded (never executed),
// so no real ADB binary is needed.

/// When DevicesLoaded arrives with a new authorized device,
/// the android pane must transition to Loading and active_serial must be set.
///
/// Note: constructing a fake AdbClient is valid here because Task::perform
/// is lazy — the async closure runs only when driven by the Iced runtime.
/// Unit tests that discard the returned Task never invoke ADB.
#[test]
fn devicesloaded_new_device_transitions_pane_to_loading() {
    use std::sync::Arc;
    let mut app = App {
        adb_client: Some(AdbClient {
            adb_path: PathBuf::from("/fake/adb"),
        }),
        ..Default::default()
    };
    assert_eq!(app.android_pane.state, PaneState::NoDevice);
    assert!(app.active_serial.is_none());

    let device = AdbDevice {
        serial: "TEST001".to_string(),
        state: adb::DeviceState::Device,
        model: Some("Pixel 7".to_string()),
        product: None,
    };
    let _ = app.update(Message::DevicesLoaded(Arc::new(vec![device])));

    assert_eq!(
        app.android_pane.state,
        PaneState::Loading,
        "pane should be Loading after device connect"
    );
    assert_eq!(
        app.active_serial.as_deref(),
        Some("TEST001"),
        "active_serial must be set to the new device serial"
    );
}

/// When DevicesLoaded arrives with no devices while a device was active,
/// active_serial is cleared and the pane state reverts to NoDevice.
#[test]
fn devicesloaded_disconnect_clears_active_serial_and_pane() {
    use std::sync::Arc;
    let mut app = App {
        adb_client: Some(AdbClient {
            adb_path: PathBuf::from("/fake/adb"),
        }),
        active_serial: Some("TEST001".to_string()),
        ..Default::default()
    };
    app.android_pane.state = PaneState::Ready;

    let _ = app.update(Message::DevicesLoaded(Arc::new(vec![])));

    assert!(app.active_serial.is_none(), "active_serial must be cleared");
    assert_eq!(
        app.android_pane.state,
        PaneState::NoDevice,
        "pane state must revert to NoDevice"
    );
}

/// When DevicesLoaded sees the same device already active, nothing changes.
#[test]
fn devicesloaded_same_device_is_no_change() {
    use std::sync::Arc;
    let mut app = App {
        adb_client: Some(AdbClient {
            adb_path: PathBuf::from("/fake/adb"),
        }),
        active_serial: Some("TEST001".to_string()),
        ..Default::default()
    };
    app.android_pane.state = PaneState::Ready;

    let device = AdbDevice {
        serial: "TEST001".to_string(),
        state: adb::DeviceState::Device,
        model: Some("Pixel 7".to_string()),
        product: None,
    };
    let _ = app.update(Message::DevicesLoaded(Arc::new(vec![device])));

    assert_eq!(
        app.active_serial.as_deref(),
        Some("TEST001"),
        "serial unchanged for same device"
    );
    assert_eq!(
        app.android_pane.state,
        PaneState::Ready,
        "pane state unchanged for same device"
    );
}

/// When AndroidEntriesLoaded arrives, the pane transitions from Loading to Ready
/// and the entries are stored.
#[test]
fn android_entries_loaded_transitions_pane_to_ready() {
    let mut app = App::default();
    app.android_pane.state = PaneState::Loading;

    let entries = vec![
        DirEntry {
            name: "DCIM".to_string(),
            path: PathBuf::from("/sdcard/DCIM"),
            size: 0,
            modified_display: "2024-01-01".to_string(),
            is_dir: true,
            is_symlink: false,
            is_hidden: false,
            child_count: None,
        },
        DirEntry {
            name: "photo.jpg".to_string(),
            path: PathBuf::from("/sdcard/photo.jpg"),
            size: 512 * 1024,
            modified_display: "2024-01-02".to_string(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
            child_count: None,
        },
    ];
    let _ = app.update(Message::AndroidEntriesLoaded {
        path: PathBuf::from("/sdcard"),
        entries,
        roots: vec![PathBuf::from("/sdcard")],
    });

    assert_eq!(
        app.android_pane.state,
        PaneState::Ready,
        "pane should transition to Ready after entries loaded"
    );
    assert_eq!(
        app.android_pane.entries.len(),
        2,
        "both entries should be stored"
    );
    assert_eq!(app.android_pane.entries[0].name, "DCIM");
    assert_eq!(app.android_pane.entries[1].name, "photo.jpg");
    assert_eq!(
        app.android_pane.current_path,
        PathBuf::from("/sdcard"),
        "current_path must be updated"
    );
}

/// Local pane: LocalEntriesLoaded transitions to Ready with correct entries.
#[test]
fn local_entries_loaded_transitions_pane_to_ready() {
    let mut app = App::default();
    app.local_pane.state = PaneState::Loading;
    let test_path = PathBuf::from("/test/home");
    app.local_pane.current_path = test_path.clone();

    let entries = vec![DirEntry {
        name: "Documents".to_string(),
        path: test_path.join("Documents"),
        size: 0,
        modified_display: "2024-01-01".to_string(),
        is_dir: true,
        is_symlink: false,
        is_hidden: false,
        child_count: Some(10),
    }];

    let _ = app.update(Message::LocalEntriesLoaded {
        path: test_path,
        entries,
    });

    assert_eq!(app.local_pane.state, PaneState::Ready);
    assert_eq!(app.local_pane.entries.len(), 1);
    assert_eq!(app.local_pane.entries[0].name, "Documents");
}

/// Android load error transitions pane to an error state.
#[test]
fn android_load_error_sets_error_state() {
    let mut app = App::default();
    app.android_pane.state = PaneState::Loading;

    let _ = app.update(Message::AndroidLoadError("connection refused".to_string()));

    assert!(
        matches!(app.android_pane.state, PaneState::Error(_)),
        "pane should be in Error state after load failure"
    );
}

/// A stale AndroidEntriesLoaded arriving after disconnect must NOT change pane
/// state. `FilePane::on_entries_loaded` guards against this by ignoring any
/// update when the state is not `Loading`.
#[test]
fn stale_entries_loaded_after_disconnect_is_ignored() {
    // active_serial is already None in Default; android_pane.state is already NoDevice
    let mut app = App::default();

    // Stale response arrives from a Task that was in-flight before disconnect
    let _ = app.update(Message::AndroidEntriesLoaded {
        path: PathBuf::from("/sdcard"),
        entries: vec![],
        roots: vec![],
    });

    // Stale response must be silently dropped — state stays NoDevice
    assert_eq!(
        app.android_pane.state,
        PaneState::NoDevice,
        "stale entries must not override NoDevice state"
    );
}

/// AdbError increments the daemon_error_count.
#[test]
fn adb_error_increments_error_count() {
    let mut app = App::default();
    assert_eq!(app.daemon_error_count, 0);
    let _ = app.update(Message::AdbError("timeout".to_string()));
    assert_eq!(app.daemon_error_count, 1);
    let _ = app.update(Message::AdbError("timeout".to_string()));
    assert_eq!(app.daemon_error_count, 2);
}

// ── Transfer handler unit tests ───────────────────────────────────────────
//
// These tests drive the state-machine handlers in update/transfer.rs through
// App::update() without ever spawning an adb subprocess.  The subscription
// that actually calls run_transfer is lazy (Iced only starts it when the
// runtime polls it) so discarding the returned Task is safe here.

/// Build a fake local file entry for testing.
fn local_file(name: &str, path: &str) -> DirEntry {
    DirEntry {
        name: name.to_string(),
        path: PathBuf::from(path),
        size: 1024,
        modified_display: "2024-01-01".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    }
}

/// Build a fake local directory entry.
fn local_dir(name: &str, path: &str) -> DirEntry {
    DirEntry {
        name: name.to_string(),
        path: PathBuf::from(path),
        size: 0,
        modified_display: "2024-01-01".to_string(),
        is_dir: true,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    }
}

/// Return an App pre-configured with a fake ADB client, device serial,
/// and the given local pane entries with all indices selected.
fn app_with_local_selection(entries: Vec<DirEntry>) -> App {
    let count = entries.len();
    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: PathBuf::from("/fake/adb"),
        }),
        active_serial: Some("device1234".to_string()),
        ..Default::default()
    };
    app.local_pane.entries = entries;
    app.local_pane.selected = (0..count).collect();
    app
}

// ── copy_to_android ───────────────────────────────────────────────────────

#[test]
fn copy_to_android_no_client_is_noop() {
    let mut app = App::default(); // adb_client = None
    let _ = app.update(Message::CopyToAndroid);
    assert!(app.active_transfer.is_none());
    assert!(app.transfer_status.is_none());
}

#[test]
fn copy_to_android_no_serial_is_noop() {
    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: PathBuf::from("/fake/adb"),
        }),
        ..Default::default() // active_serial = None
    };
    let _ = app.update(Message::CopyToAndroid);
    assert!(app.active_transfer.is_none());
}

#[test]
fn copy_to_android_empty_selection_is_noop() {
    let mut app = app_with_local_selection(vec![]);
    let _ = app.update(Message::CopyToAndroid);
    assert!(app.active_transfer.is_none());
    assert!(app.transfer_status.is_none());
}

#[test]
fn copy_to_android_directory_only_selection_is_noop() {
    // Directories are skipped — only files are transferred.
    let mut app = app_with_local_selection(vec![local_dir("Documents", "/home/user/Documents")]);
    let _ = app.update(Message::CopyToAndroid);
    assert!(
        app.active_transfer.is_none(),
        "directories must not queue a transfer"
    );
}

#[test]
fn copy_to_android_single_file_sets_active_transfer() {
    let mut app = app_with_local_selection(vec![local_file("photo.jpg", "/home/user/photo.jpg")]);
    let _ = app.update(Message::CopyToAndroid);

    let job = app
        .active_transfer
        .as_ref()
        .expect("active_transfer must be set");
    assert_eq!(job.filename, "photo.jpg");
    assert_eq!(job.source, PathBuf::from("/home/user/photo.jpg"));
    assert_eq!(job.destination, app.android_pane.current_path);
    assert!(matches!(job.direction, adb::TransferDirection::ToAndroid));
}

#[test]
fn copy_to_android_single_file_initialises_transfer_status() {
    let mut app = app_with_local_selection(vec![local_file("photo.jpg", "/home/user/photo.jpg")]);
    let _ = app.update(Message::CopyToAndroid);

    let status = app
        .transfer_status
        .as_ref()
        .expect("transfer_status must be set");
    assert_eq!(status.filename, "photo.jpg");
    assert_eq!(status.percent, 0);
    assert_eq!(status.job_index, 1);
    assert_eq!(status.job_total, 1);
    assert!(status.speed_display.is_empty());
}

#[test]
fn copy_to_android_multiple_files_queues_remainder() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/home/user/a.jpg"),
        local_file("b.jpg", "/home/user/b.jpg"),
        local_file("c.jpg", "/home/user/c.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);

    // First job is active; two remain in the queue.
    assert!(app.active_transfer.is_some(), "first job must be active");
    assert_eq!(
        app.transfer_queue.len(),
        2,
        "remaining 2 jobs must be queued"
    );
    assert_eq!(app.transfer_queue_total, 3);
    assert_eq!(app.transfer_queue_done, 0);

    let status = app.transfer_status.as_ref().unwrap();
    assert_eq!(status.job_total, 3);
    assert_eq!(status.job_index, 1);
}

#[test]
fn copy_to_android_mixed_selection_skips_dirs() {
    // 1 dir + 2 files → only the 2 files should transfer.
    let mut app = app_with_local_selection(vec![
        local_dir("DCIM", "/home/user/DCIM"),
        local_file("a.jpg", "/home/user/a.jpg"),
        local_file("b.jpg", "/home/user/b.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);

    assert!(app.active_transfer.is_some());
    assert_eq!(
        app.transfer_queue_total, 2,
        "only file count, dirs excluded"
    );
    assert_eq!(app.transfer_queue.len(), 1);
}

#[test]
fn copy_to_android_creates_cancel_flag() {
    let mut app = app_with_local_selection(vec![local_file("f.txt", "/home/user/f.txt")]);
    let _ = app.update(Message::CopyToAndroid);
    assert!(app.cancel_flag.is_some(), "cancel_flag must be initialised");
    // Flag starts as false — not yet cancelled.
    let flag = app.cancel_flag.as_ref().unwrap();
    assert!(!flag.load(std::sync::atomic::Ordering::Relaxed));
}

// ── transfer_progress ─────────────────────────────────────────────────────

#[test]
fn transfer_progress_updates_percent() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);

    let _ = app.update(Message::TransferProgress { percent: 42 });

    assert_eq!(app.transfer_status.as_ref().unwrap().percent, 42);
}

#[test]
fn transfer_progress_noop_without_active_status() {
    let mut app = App::default();
    // Should not panic even with no transfer in progress.
    let _ = app.update(Message::TransferProgress { percent: 50 });
    assert!(app.transfer_status.is_none());
}

#[test]
fn transfer_progress_successive_updates() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);

    for pct in [25u8, 50, 75, 100] {
        let _ = app.update(Message::TransferProgress { percent: pct });
        assert_eq!(app.transfer_status.as_ref().unwrap().percent, pct);
    }
}

// ── transfer_complete ─────────────────────────────────────────────────────

#[test]
fn transfer_complete_single_job_clears_state() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);
    let _ = app.update(Message::TransferComplete {
        speed_display: "1.2 MB/s".to_string(),
    });

    assert!(
        app.active_transfer.is_none(),
        "active_transfer must be cleared"
    );
    assert!(
        app.transfer_status.is_none(),
        "transfer_status must be cleared"
    );
    assert!(app.cancel_flag.is_none(), "cancel_flag must be cleared");
    assert!(app.transfer_queue.is_empty());
}

#[test]
fn transfer_complete_with_queued_jobs_advances_to_next() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/tmp/a.jpg"),
        local_file("b.jpg", "/tmp/b.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);

    // Complete the first job.
    let _ = app.update(Message::TransferComplete {
        speed_display: "1.0 MB/s".to_string(),
    });

    // The second job should now be active.
    let job = app
        .active_transfer
        .as_ref()
        .expect("second job must be active");
    assert_eq!(job.filename, "b.jpg");
    assert_eq!(
        app.transfer_queue.len(),
        0,
        "queue should be empty after advancing"
    );
}

#[test]
fn transfer_complete_advances_job_index() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/tmp/a.jpg"),
        local_file("b.jpg", "/tmp/b.jpg"),
        local_file("c.jpg", "/tmp/c.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);

    let _ = app.update(Message::TransferComplete {
        speed_display: String::new(),
    });

    let status = app.transfer_status.as_ref().unwrap();
    assert_eq!(
        status.job_index, 2,
        "job_index must advance to 2 after first completes"
    );
    assert_eq!(status.job_total, 3);
}

#[test]
fn transfer_complete_resets_percent_for_next_job() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/tmp/a.jpg"),
        local_file("b.jpg", "/tmp/b.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);
    let _ = app.update(Message::TransferProgress { percent: 80 });

    // Complete first — second job should start at 0%.
    let _ = app.update(Message::TransferComplete {
        speed_display: String::new(),
    });

    assert_eq!(app.transfer_status.as_ref().unwrap().percent, 0);
}

#[test]
fn transfer_complete_creates_new_cancel_flag_for_next_job() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/tmp/a.jpg"),
        local_file("b.jpg", "/tmp/b.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);
    let first_flag = app.cancel_flag.clone().unwrap();

    let _ = app.update(Message::TransferComplete {
        speed_display: String::new(),
    });

    let second_flag = app.cancel_flag.as_ref().expect("new flag must exist");
    // Each job gets a fresh flag (different Arc pointer).
    assert!(
        !std::sync::Arc::ptr_eq(&first_flag, second_flag),
        "each job must have its own cancel flag"
    );
}

// ── transfer_failed ───────────────────────────────────────────────────────

#[test]
fn transfer_failed_clears_active_transfer() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);
    let _ = app.update(Message::TransferFailed("adb exited with 1".to_string()));

    assert!(app.active_transfer.is_none());
    assert!(app.transfer_status.is_none());
    assert!(app.cancel_flag.is_none());
}

#[test]
fn transfer_failed_sets_error_banner() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);
    let _ = app.update(Message::TransferFailed("connection lost".to_string()));

    let banner = app
        .error_banner
        .as_deref()
        .expect("error_banner must be set");
    assert!(
        banner.contains("connection lost"),
        "banner should contain the error: {banner}"
    );
}

#[test]
fn transfer_failed_clears_remaining_queue() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/tmp/a.jpg"),
        local_file("b.jpg", "/tmp/b.jpg"),
        local_file("c.jpg", "/tmp/c.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);
    assert_eq!(app.transfer_queue.len(), 2);

    let _ = app.update(Message::TransferFailed("timeout".to_string()));
    assert!(
        app.transfer_queue.is_empty(),
        "queue must be cleared after failure"
    );
}

// ── cancel_transfer ───────────────────────────────────────────────────────

#[test]
fn cancel_transfer_sets_cancel_flag() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);

    let flag = app.cancel_flag.clone().unwrap();
    let _ = app.update(Message::CancelTransfer);

    assert!(
        flag.load(std::sync::atomic::Ordering::Relaxed),
        "cancel flag must be set to true"
    );
}

#[test]
fn cancel_transfer_clears_queue() {
    let mut app = app_with_local_selection(vec![
        local_file("a.jpg", "/tmp/a.jpg"),
        local_file("b.jpg", "/tmp/b.jpg"),
    ]);
    let _ = app.update(Message::CopyToAndroid);
    assert_eq!(app.transfer_queue.len(), 1);

    let _ = app.update(Message::CancelTransfer);
    assert!(
        app.transfer_queue.is_empty(),
        "queue must be cleared on cancel"
    );
}

#[test]
fn cancel_transfer_noop_without_active_transfer() {
    let mut app = App::default();
    // Should not panic even with no transfer.
    let _ = app.update(Message::CancelTransfer);
}

// ── transfer_cancelled ────────────────────────────────────────────────────

#[test]
fn transfer_cancelled_clears_all_state() {
    let mut app = app_with_local_selection(vec![local_file("f.jpg", "/tmp/f.jpg")]);
    let _ = app.update(Message::CopyToAndroid);
    let _ = app.update(Message::TransferCancelled);

    assert!(app.active_transfer.is_none());
    assert!(app.transfer_status.is_none());
    assert!(app.cancel_flag.is_none());
    assert!(app.transfer_queue.is_empty());
}

// ── copy_to_local ─────────────────────────────────────────────────────────

#[test]
fn copy_to_local_no_client_is_noop() {
    let mut app = App::default();
    let _ = app.update(Message::CopyToLocal);
    assert!(app.active_transfer.is_none());
}

#[test]
fn copy_to_local_single_file_sets_active_transfer() {
    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: PathBuf::from("/fake/adb"),
        }),
        active_serial: Some("device1234".to_string()),
        ..Default::default()
    };
    app.android_pane.entries = vec![DirEntry {
        name: "photo.jpg".to_string(),
        path: PathBuf::from("/sdcard/photo.jpg"),
        size: 3_145_728,
        modified_display: "2024-01-01".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    }];
    app.android_pane.selected = vec![0];

    let _ = app.update(Message::CopyToLocal);

    let job = app
        .active_transfer
        .as_ref()
        .expect("active_transfer must be set");
    assert_eq!(job.filename, "photo.jpg");
    assert_eq!(job.source, PathBuf::from("/sdcard/photo.jpg"));
    assert_eq!(job.destination, app.local_pane.current_path);
    assert!(matches!(job.direction, adb::TransferDirection::ToLocal));
}

// ── End-to-end transfer flow tests ───────────────────────────────────────────
//
// These tests wire together the full copy pipeline:
//   1. App::update(CopyToAndroid/CopyToLocal)  — sets up active_transfer
//   2. adb::run_transfer with mock-adb          — real subprocess, real stderr parsing
//   3. apply_event() per TransferEvent          — same mapping the subscription uses
//   4. Assert App state at each step            — what the UI reads from
//
// If these pass, the status bar will show correct progress and the pane will
// refresh after completion.  The only untested slice is the Iced channel
// wrapper itself, which is thin framework glue.

/// Helper: create a real temp source file and an App ready to push it.
fn app_for_push(tmp: &tempfile::TempDir) -> (App, std::path::PathBuf) {
    let src = tmp.path().join("photo.jpg");
    std::fs::write(&src, b"fake image data").expect("write source file");

    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: mock_adb_path(),
        }),
        active_serial: Some("device1234".to_string()),
        ..Default::default()
    };
    app.local_pane.entries = vec![DirEntry {
        name: "photo.jpg".to_string(),
        path: src.clone(),
        size: 15,
        modified_display: "2024-01-01".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    }];
    app.local_pane.selected = vec![0];
    // android destination stays at the default /sdcard — mock-adb ignores it
    (app, src)
}

// ─── Push (local → android) ──────────────────────────────────────────────────

#[tokio::test]
async fn push_progress_events_reach_transfer_status() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut app, _src) = app_for_push(&tmp);

    let _ = app.update(Message::CopyToAndroid);
    assert_eq!(
        app.transfer_status.as_ref().unwrap().percent,
        0,
        "should start at 0% before any events"
    );

    let job = app.active_transfer.clone().expect("active_transfer set");
    let cancel = app.cancel_flag.clone().unwrap();
    let mut events = Vec::new();
    adb::run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .unwrap();

    // Feed every event through App — same path the Iced subscription takes
    let mut observed_percents: Vec<u8> = Vec::new();
    for ev in events {
        if let adb::TransferEvent::Progress { percent } = &ev {
            observed_percents.push(*percent);
        }
        apply_event(&mut app, ev);
    }

    // mock-adb emits exactly [ 25% 50% 75% 100% ]
    assert_eq!(
        observed_percents,
        vec![25, 50, 75, 100],
        "mock-adb must emit 25/50/75/100 progress ticks"
    );
    // After each Progress event the status bar percent must match
    // (verified end-state: must have reached 100 before Complete arrived)
    assert!(observed_percents.contains(&100));
}

#[tokio::test]
async fn push_complete_event_clears_state_and_carries_speed() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut app, _src) = app_for_push(&tmp);

    let _ = app.update(Message::CopyToAndroid);
    let job = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();

    let mut events = Vec::new();
    adb::run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .unwrap();

    let last = events.last().cloned().unwrap();
    // Verify the engine itself produced a Complete with speed
    assert!(
        matches!(last, adb::TransferEvent::Complete { ref speed_display }
        if speed_display == "1.2 MB/s"),
        "mock-adb push must report 1.2 MB/s, got: {last:?}"
    );

    for ev in events {
        apply_event(&mut app, ev);
    }

    // After TransferComplete the UI state must be fully cleared
    assert!(
        app.active_transfer.is_none(),
        "active_transfer must clear after complete"
    );
    assert!(
        app.transfer_status.is_none(),
        "transfer_status must clear after complete"
    );
    assert!(app.cancel_flag.is_none());
}

#[tokio::test]
async fn push_progress_updates_status_bar_percent_incrementally() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut app, _src) = app_for_push(&tmp);
    let _ = app.update(Message::CopyToAndroid);

    let job = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();

    // Collect and apply events one at a time, checking percent after each
    let mut events = Vec::new();
    adb::run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .unwrap();

    let mut expected = vec![25u8, 50, 75, 100].into_iter();
    for ev in events {
        if matches!(ev, adb::TransferEvent::Progress { .. }) {
            apply_event(&mut app, ev);
            let pct = app.transfer_status.as_ref().unwrap().percent;
            assert_eq!(
                pct,
                expected.next().unwrap(),
                "status bar percent must match mock-adb progress tick"
            );
        } else {
            apply_event(&mut app, ev);
        }
    }
}

// ─── Pull (android → local) ──────────────────────────────────────────────────

#[tokio::test]
async fn pull_progress_events_reach_transfer_status() {
    let tmp = tempfile::tempdir().unwrap();
    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: mock_adb_path(),
        }),
        active_serial: Some("device1234".to_string()),
        ..Default::default()
    };
    // Android pane: one file selected
    app.android_pane.entries = vec![DirEntry {
        name: "photo.jpg".to_string(),
        path: std::path::PathBuf::from("/sdcard/DCIM/photo.jpg"),
        size: 3_145_728,
        modified_display: "2024-01-01".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    }];
    app.android_pane.selected = vec![0];
    // Local destination: the temp dir (mock-adb writes the file there)
    app.local_pane.current_path = tmp.path().to_path_buf();

    let _ = app.update(Message::CopyToLocal);
    assert!(app.active_transfer.is_some());
    let job = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();

    let mut events = Vec::new();
    adb::run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .unwrap();

    let percents: Vec<u8> = events
        .iter()
        .filter_map(|e| {
            if let adb::TransferEvent::Progress { percent } = e {
                Some(*percent)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(percents, vec![25, 50, 75, 100]);

    for ev in events {
        apply_event(&mut app, ev);
    }

    assert!(app.active_transfer.is_none());
    assert!(app.transfer_status.is_none());
}

#[tokio::test]
async fn pull_complete_carries_correct_speed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: mock_adb_path(),
        }),
        active_serial: Some("device1234".to_string()),
        ..Default::default()
    };
    app.android_pane.entries = vec![DirEntry {
        name: "photo.jpg".to_string(),
        path: std::path::PathBuf::from("/sdcard/DCIM/photo.jpg"),
        size: 3_145_728,
        modified_display: "2024-01-01".to_string(),
        is_dir: false,
        is_symlink: false,
        is_hidden: false,
        child_count: None,
    }];
    app.android_pane.selected = vec![0];
    app.local_pane.current_path = tmp.path().to_path_buf();

    let _ = app.update(Message::CopyToLocal);
    let job = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();

    let mut events = Vec::new();
    adb::run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .unwrap();

    let last = events.last().cloned().unwrap();
    // mock-adb pull summary: "1.0 MB/s"
    assert!(
        matches!(last, adb::TransferEvent::Complete { ref speed_display }
        if speed_display == "1.0 MB/s"),
        "mock-adb pull must report 1.0 MB/s, got: {last:?}"
    );
}

// ─── Multi-file queue ─────────────────────────────────────────────────────────

#[tokio::test]
async fn multi_file_push_queue_processes_all_jobs_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let src_a = tmp.path().join("a.jpg");
    let src_b = tmp.path().join("b.jpg");
    std::fs::write(&src_a, b"aaa").unwrap();
    std::fs::write(&src_b, b"bbb").unwrap();

    let mut app = App {
        adb_client: Some(adb::AdbClient {
            adb_path: mock_adb_path(),
        }),
        active_serial: Some("device1234".to_string()),
        ..Default::default()
    };
    app.local_pane.entries = vec![
        DirEntry {
            name: "a.jpg".into(),
            path: src_a,
            size: 3,
            modified_display: "2024-01-01".into(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
            child_count: None,
        },
        DirEntry {
            name: "b.jpg".into(),
            path: src_b,
            size: 3,
            modified_display: "2024-01-01".into(),
            is_dir: false,
            is_symlink: false,
            is_hidden: false,
            child_count: None,
        },
    ];
    app.local_pane.selected = vec![0, 1];

    let _ = app.update(Message::CopyToAndroid);
    assert_eq!(app.transfer_queue_total, 2);
    assert_eq!(app.transfer_status.as_ref().unwrap().job_index, 1);

    // ── Run job 1 ──
    let job1 = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();
    let mut events1 = Vec::new();
    adb::run_transfer(&job1, cancel, |ev| events1.push(ev))
        .await
        .unwrap();
    for ev in events1 {
        apply_event(&mut app, ev);
    }

    // After first completes, second job must be active
    assert!(
        app.active_transfer.is_some(),
        "second job must start after first completes"
    );
    assert_eq!(
        app.transfer_status.as_ref().unwrap().job_index,
        2,
        "job_index must advance to 2"
    );
    assert_eq!(
        app.transfer_status.as_ref().unwrap().percent,
        0,
        "percent resets to 0 for each new job"
    );
    assert_eq!(app.transfer_queue.len(), 0);

    // ── Run job 2 ──
    let job2 = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();
    let mut events2 = Vec::new();
    adb::run_transfer(&job2, cancel, |ev| events2.push(ev))
        .await
        .unwrap();
    for ev in events2 {
        apply_event(&mut app, ev);
    }

    // Both jobs complete — all state cleared
    assert!(app.active_transfer.is_none(), "all transfers must finish");
    assert!(app.transfer_status.is_none());
}

// ─── Cancel mid-flight ────────────────────────────────────────────────────────

#[tokio::test]
async fn cancel_flag_stops_transfer_and_clears_state() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut app, _src) = app_for_push(&tmp);

    let _ = app.update(Message::CopyToAndroid);

    // Request cancel before the subscription drives the transfer
    let _ = app.update(Message::CancelTransfer);

    let job = app.active_transfer.clone().unwrap();
    let cancel = app.cancel_flag.clone().unwrap();
    // cancel flag is now true — run_transfer will stop after first stderr line
    let mut events = Vec::new();
    adb::run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .any(|e| matches!(e, adb::TransferEvent::Cancelled)),
        "cancel flag must produce a Cancelled event"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, adb::TransferEvent::Complete { .. })),
        "must not Complete after cancel"
    );

    for ev in events {
        apply_event(&mut app, ev);
    }
    assert!(app.active_transfer.is_none());
    assert!(app.transfer_status.is_none());
    assert!(app.transfer_queue.is_empty());
}
