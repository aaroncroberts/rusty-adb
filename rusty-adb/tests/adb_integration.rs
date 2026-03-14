//! Integration tests for AdbClient and run_transfer using the mock-adb binary.
//!
//! Each test spawns `tests/fixtures/mock-adb` (a bash script with canned
//! responses) and exercises the full command → subprocess → parse pipeline
//! without a physical Android device.
//!
//! # Why integration tests?
//! Unit tests in `adb.rs` cover the parsers in isolation.  These integration
//! tests verify that `AdbClient` builds the *correct command-line arguments*,
//! reads the subprocess output, and returns the *correct domain types*.  Any
//! argument ordering bug or stderr/stdout confusion would only surface here.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusty_adb::adb::{AdbClient, DeviceState};
use rusty_adb::adb::{run_transfer, TransferDirection, TransferEvent, TransferJob};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Absolute path to the mock-adb fixture script.
///
/// `CARGO_MANIFEST_DIR` is set by Cargo to the crate root at test time,
/// so this path is always correct regardless of where `cargo test` is run.
fn mock_adb_path() -> PathBuf {
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by Cargo");
    PathBuf::from(manifest).join("tests/fixtures/mock-adb")
}

fn make_client() -> AdbClient {
    AdbClient {
        adb_path: mock_adb_path(),
    }
}

// ── Device listing ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_devices_returns_both_devices() {
    let client = make_client();
    let devices = client.list_devices().await.expect("list_devices failed");

    assert_eq!(devices.len(), 2, "mock returns exactly 2 devices");
}

#[tokio::test]
async fn test_list_devices_authorized_device() {
    let client = make_client();
    let devices = client.list_devices().await.expect("list_devices failed");

    let dev = devices
        .iter()
        .find(|d| d.serial == "device1234")
        .expect("device1234 should be present");

    assert_eq!(dev.state, DeviceState::Device);
    // model:Pixel_7 → underscores replaced with spaces
    assert_eq!(dev.model.as_deref(), Some("Pixel 7"));
    assert_eq!(dev.product.as_deref(), Some("panther"));
}

#[tokio::test]
async fn test_list_devices_unauthorized_device() {
    let client = make_client();
    let devices = client.list_devices().await.expect("list_devices failed");

    let dev = devices
        .iter()
        .find(|d| d.serial == "unauth5678")
        .expect("unauth5678 should be present");

    assert_eq!(dev.state, DeviceState::Unauthorized);
    assert!(dev.model.is_none(), "unauthorized device has no model");
}

// ── Directory listing ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_dir_sdcard_entry_count() {
    let client = make_client();
    let path = std::path::Path::new("/sdcard");
    let entries = client
        .list_dir("device1234", path)
        .await
        .expect("list_dir failed");

    // DCIM (dir), Download (dir), readme.txt (file) — dot entries filtered
    assert_eq!(entries.len(), 3, "expected DCIM + Download + readme.txt");
}

#[tokio::test]
async fn test_list_dir_sdcard_directories() {
    let client = make_client();
    let path = std::path::Path::new("/sdcard");
    let entries = client
        .list_dir("device1234", path)
        .await
        .expect("list_dir failed");

    for name in &["DCIM", "Download"] {
        let entry = entries
            .iter()
            .find(|e| e.name == *name)
            .unwrap_or_else(|| panic!("{name} not found in listing"));
        assert!(entry.is_dir, "{name} should be a directory");
        assert_eq!(entry.path, path.join(name));
    }
}

#[tokio::test]
async fn test_list_dir_sdcard_file_metadata() {
    let client = make_client();
    let path = std::path::Path::new("/sdcard");
    let entries = client
        .list_dir("device1234", path)
        .await
        .expect("list_dir failed");

    let readme = entries
        .iter()
        .find(|e| e.name == "readme.txt")
        .expect("readme.txt not found");

    assert!(!readme.is_dir);
    assert_eq!(readme.size, 102_400);
    assert_eq!(readme.modified, "2024-01-15");
}

#[tokio::test]
async fn test_list_dir_dcim_entry_count() {
    let client = make_client();
    let path = std::path::Path::new("/sdcard/DCIM");
    let entries = client
        .list_dir("device1234", path)
        .await
        .expect("list_dir DCIM failed");

    assert_eq!(entries.len(), 2, "expected photo.jpg + photo2.jpg");
}

#[tokio::test]
async fn test_list_dir_dcim_file_sizes() {
    let client = make_client();
    let path = std::path::Path::new("/sdcard/DCIM");
    let entries = client
        .list_dir("device1234", path)
        .await
        .expect("list_dir DCIM failed");

    let photo = entries
        .iter()
        .find(|e| e.name == "photo.jpg")
        .expect("photo.jpg not found");
    assert_eq!(photo.size, 3_145_728);

    let photo2 = entries
        .iter()
        .find(|e| e.name == "photo2.jpg")
        .expect("photo2.jpg not found");
    assert_eq!(photo2.size, 2_097_152);
}

// ── Transfer progress ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_transfer_progress_events_received() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");

    let job = TransferJob {
        id: 1,
        adb_path: mock_adb_path(),
        serial: "device1234".to_string(),
        source: PathBuf::from("/sdcard/DCIM/photo.jpg"),
        destination: tmp.path().join("photo.jpg"),
        direction: TransferDirection::ToLocal,
        filename: "photo.jpg".to_string(),
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let mut events: Vec<TransferEvent> = Vec::new();

    run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .expect("run_transfer failed");

    let progress_percents: Vec<u8> = events
        .iter()
        .filter_map(|e| {
            if let TransferEvent::Progress { percent } = e {
                Some(*percent)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !progress_percents.is_empty(),
        "expected at least one Progress event"
    );
    assert!(
        progress_percents.contains(&25),
        "expected 25% progress event"
    );
    assert!(
        progress_percents.contains(&100),
        "expected 100% progress event"
    );
}

#[tokio::test]
async fn test_transfer_completes_successfully() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");

    let job = TransferJob {
        id: 2,
        adb_path: mock_adb_path(),
        serial: "device1234".to_string(),
        source: PathBuf::from("/sdcard/DCIM/photo.jpg"),
        destination: tmp.path().join("photo.jpg"),
        direction: TransferDirection::ToLocal,
        filename: "photo.jpg".to_string(),
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let mut events: Vec<TransferEvent> = Vec::new();

    run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .expect("run_transfer failed");

    let last = events.last().expect("no events emitted");
    assert!(
        matches!(last, TransferEvent::Complete { .. }),
        "last event should be Complete, got: {last:?}"
    );
}

#[tokio::test]
async fn test_transfer_cancel_stops_early() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");

    let job = TransferJob {
        id: 3,
        adb_path: mock_adb_path(),
        serial: "device1234".to_string(),
        source: PathBuf::from("/sdcard/DCIM/photo.jpg"),
        destination: tmp.path().join("photo_cancelled.jpg"),
        direction: TransferDirection::ToLocal,
        filename: "photo.jpg".to_string(),
    };

    // Set cancel = true before even starting so the first line check fires
    let cancel = Arc::new(AtomicBool::new(true));
    let mut events: Vec<TransferEvent> = Vec::new();

    run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .expect("run_transfer failed");

    assert!(
        events.iter().any(|e| matches!(e, TransferEvent::Cancelled)),
        "expected a Cancelled event when cancel flag is pre-set"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, TransferEvent::Complete { .. })),
        "should not receive Complete when cancelled"
    );
}

#[tokio::test]
async fn test_push_transfer_progress_events() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");

    // Create a dummy source file for the push
    let src = tmp.path().join("local_file.txt");
    std::fs::write(&src, b"test content").expect("failed to write source file");

    let job = TransferJob {
        id: 4,
        adb_path: mock_adb_path(),
        serial: "device1234".to_string(),
        source: src,
        destination: PathBuf::from("/sdcard/local_file.txt"),
        direction: TransferDirection::ToAndroid,
        filename: "local_file.txt".to_string(),
    };

    let cancel = Arc::new(AtomicBool::new(false));
    let mut events: Vec<TransferEvent> = Vec::new();

    run_transfer(&job, cancel, |ev| events.push(ev))
        .await
        .expect("run_transfer push failed");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferEvent::Progress { .. })),
        "push should emit Progress events"
    );
    assert!(
        matches!(events.last(), Some(TransferEvent::Complete { .. })),
        "push should end with Complete"
    );
}

// ── File operations on device ─────────────────────────────────────────────────

#[tokio::test]
async fn test_rename_succeeds() {
    let client = make_client();
    let from = std::path::Path::new("/sdcard/readme.txt");
    let to = std::path::Path::new("/sdcard/readme2.txt");
    client
        .rename("device1234", from, to)
        .await
        .expect("rename should succeed with mock-adb");
}

#[tokio::test]
async fn test_delete_succeeds() {
    let client = make_client();
    let path = std::path::Path::new("/sdcard/readme.txt");
    client
        .delete("device1234", path)
        .await
        .expect("delete should succeed with mock-adb");
}

#[tokio::test]
async fn test_pull_to_temp_returns_existing_file() {
    let client = make_client();
    let remote = std::path::Path::new("/sdcard/DCIM/photo.jpg");

    let local_path = client
        .pull_to_temp("device1234", remote)
        .await
        .expect("pull_to_temp should succeed with mock-adb");

    assert!(
        local_path.exists(),
        "pull_to_temp must create a file at the local path"
    );
    assert_eq!(
        local_path.file_name().and_then(|n| n.to_str()),
        Some("photo.jpg"),
        "filename should match the remote filename"
    );
}

// ── Lifecycle commands ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_start_server_succeeds() {
    let client = make_client();
    // start_server returns Ok(()) when adb exits 0
    client
        .start_server()
        .await
        .expect("start_server should succeed with mock-adb");
}

#[tokio::test]
async fn test_disconnect_succeeds() {
    let client = make_client();
    // disconnect takes a serial — mock-adb prints "disconnected <serial>" and exits 0
    client
        .disconnect("device1234")
        .await
        .expect("disconnect should succeed with mock-adb");
}
