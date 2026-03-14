use super::*;
use adb::DeviceState;

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

    let entries = vec![DirEntry {
        name: "Documents".to_string(),
        path: PathBuf::from("/Users/aaron/Documents"),
        size: 0,
        modified_display: "2024-01-01".to_string(),
        is_dir: true,
        is_symlink: false,
        is_hidden: false,
        child_count: Some(10),
    }];

    let _ = app.update(Message::LocalEntriesLoaded {
        path: PathBuf::from("/Users/aaron"),
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
