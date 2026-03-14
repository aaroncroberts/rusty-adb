//! Async file transfer engine for rusty-adb
//!
//! Uses `adb push` / `adb pull --progress` and streams stderr line-by-line
//! to produce `TransferEvent` values that drive the status bar progress display.
//!
//! # Progress format (adb stderr)
//! ```text
//! [ 12%] /sdcard/DCIM/photo.jpg
//! [ 45%] /sdcard/DCIM/photo.jpg
//! [100%] /sdcard/DCIM/photo.jpg
//! photo.jpg: 1 file pushed, 0 skipped. 12.3 MB/s (3145728 bytes in 0.244s)
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, BufReader};

// ─── Direction ─────────────────────────────────────────────────────────────────

/// Direction of a file transfer relative to the Android device
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferDirection {
    /// Local → Android device (`adb push`)
    ToAndroid,
    /// Android device → Local (`adb pull`)
    ToLocal,
}

// ─── Job ───────────────────────────────────────────────────────────────────────

/// A single pending or active file transfer
#[derive(Debug, Clone)]
pub struct TransferJob {
    /// Monotonic ID for deduplicating Iced subscriptions
    pub id: u64,
    /// ADB binary path
    pub adb_path: PathBuf,
    /// Device serial
    pub serial: String,
    /// Source path (local for push, android for pull)
    pub source: PathBuf,
    /// Destination path (android for push, local for pull)
    pub destination: PathBuf,
    /// Which direction the transfer runs (determines whether adb push or pull is used)
    pub direction: TransferDirection,
    /// Display name (source filename) — copied into `TransferStatus` at transfer start
    #[allow(dead_code)] // read in main.rs to construct TransferStatus
    pub filename: String,
}

// ─── Events ────────────────────────────────────────────────────────────────────

/// Events produced by the transfer subscription
#[derive(Debug, Clone)]
pub enum TransferEvent {
    /// Transfer in flight — percent complete (0-100)
    Progress { percent: u8 },
    /// Transfer finished successfully
    Complete { speed_display: String },
    /// Transfer was cancelled by the user
    Cancelled,
    /// Transfer failed with a human-readable error
    Failed(String),
}

// ─── Transfer Status ───────────────────────────────────────────────────────────

/// Live transfer state used by the UI during an active copy operation.
///
/// Constructed in `update/transfer.rs` from incoming [`TransferEvent`]s and
/// stored on `App` so the status bar view can render progress without
/// coupling to the event stream directly.
#[derive(Debug, Clone)]
pub struct TransferStatus {
    /// Display name of the file being transferred
    pub filename: String,
    /// Progress 0–100
    pub percent: u8,
    /// Human-readable speed e.g. "12.3 MB/s" (empty until adb reports it)
    pub speed_display: String,
    /// 1-based index of the current job in the queue
    pub job_index: usize,
    /// Total jobs in the queue
    pub job_total: usize,
}

// ─── Parsers ───────────────────────────────────────────────────────────────────

/// Parse a progress line from `adb push/pull --progress` stderr.
///
/// Handles:
/// - `[ 12%] filename`   (single space padding)
/// - `[100%] filename`
///
/// Returns the percentage as u8 on success.
pub fn parse_progress_line(line: &str) -> Option<u8> {
    let line = line.trim();
    // Must start with '[' and contain '%]'
    let inner = line.strip_prefix('[')?;
    let pct_end = inner.find("%]")?;
    let pct_str = inner[..pct_end].trim();
    pct_str.parse::<u8>().ok()
}

/// Parse the completion summary line produced by adb push/pull.
///
/// Sample: `"photo.jpg: 1 file pushed, 0 skipped. 12.3 MB/s (…)"`
/// Returns the speed token (e.g. `"12.3 MB/s"`) for display.
pub fn parse_speed(line: &str) -> Option<String> {
    // Speed is the token immediately before "(", containing "MB/s", "KB/s", or "B/s"
    let paren = line.find('(')?;
    let before_paren = &line[..paren];
    for token in before_paren
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
    {
        if token[1].ends_with("/s") || token[1] == "MB/s" || token[1] == "KB/s" || token[1] == "B/s"
        {
            return Some(format!("{} {}", token[0], token[1]));
        }
    }
    None
}

// ─── Transfer runner ───────────────────────────────────────────────────────────

/// Run a single transfer job, emitting events via the provided callback.
///
/// `cancel` is an `Arc<AtomicBool>` shared with the caller; setting it to `true`
/// from another task will cause the transfer to stop cleanly after the current
/// stderr line is processed and kill the child adb process.
pub async fn run_transfer<F>(job: &TransferJob, cancel: Arc<AtomicBool>, mut emit: F) -> Result<()>
where
    F: FnMut(TransferEvent),
{
    tracing::info!(
        direction = ?job.direction,
        source = %job.source.display(),
        destination = %job.destination.display(),
        serial = %job.serial,
        "starting transfer"
    );

    let (adb_verb, source_str, dest_str) = match job.direction {
        TransferDirection::ToAndroid => (
            "push",
            job.source.to_string_lossy().into_owned(),
            job.destination.to_string_lossy().into_owned(),
        ),
        TransferDirection::ToLocal => (
            "pull",
            job.source.to_string_lossy().into_owned(),
            job.destination.to_string_lossy().into_owned(),
        ),
    };

    // Use "-t <id>" for transport-addressed devices (serial stored as "t:<id>"),
    // otherwise use the standard "-s <serial>" selector.
    let (flag, id) = if let Some(tid) = job.serial.strip_prefix("t:") {
        ("-t", tid)
    } else {
        ("-s", job.serial.as_str())
    };

    let mut child = tokio::process::Command::new(&job.adb_path)
        .args([flag, id, adb_verb, "--progress", &source_str, &dest_str])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn adb: {e}"))?;

    let stderr = child.stderr.take().expect("stderr piped");
    let mut lines = BufReader::new(stderr).lines();

    let mut last_percent: u8 = 0;
    let mut speed_display = String::new();

    while let Ok(Some(line)) = lines.next_line().await {
        // Check cancel flag after each line — zero cost when not cancelled
        if cancel.load(Ordering::Relaxed) {
            tracing::info!("transfer cancelled by user");
            let _ = child.kill().await;
            emit(TransferEvent::Cancelled);
            return Ok(());
        }

        tracing::trace!(line = %line, "adb stderr");

        if let Some(pct) = parse_progress_line(&line) {
            if pct != last_percent {
                last_percent = pct;
                emit(TransferEvent::Progress { percent: pct });
            }
        } else if let Some(speed) = parse_speed(&line) {
            speed_display = speed;
        }
    }

    // One final cancel check before reporting success
    if cancel.load(Ordering::Relaxed) {
        tracing::info!("transfer cancelled after completion");
        emit(TransferEvent::Cancelled);
        return Ok(());
    }

    let status = child.wait().await?;
    if status.success() {
        tracing::info!(speed = %speed_display, "transfer complete");
        emit(TransferEvent::Complete { speed_display });
    } else {
        let msg = format!("adb exited with {status}");
        tracing::warn!(error = %msg, "transfer failed");
        emit(TransferEvent::Failed(msg));
    }

    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progress_single_digit_padded() {
        assert_eq!(parse_progress_line("[ 12%] /sdcard/photo.jpg"), Some(12));
    }

    #[test]
    fn parse_progress_hundred() {
        assert_eq!(parse_progress_line("[100%] /sdcard/photo.jpg"), Some(100));
    }

    #[test]
    fn parse_progress_zero() {
        assert_eq!(parse_progress_line("[  0%] /sdcard/photo.jpg"), Some(0));
    }

    #[test]
    fn parse_progress_leading_whitespace() {
        assert_eq!(parse_progress_line("  [ 55%] file.mp4  "), Some(55));
    }

    #[test]
    fn parse_progress_ignores_summary_line() {
        // summary lines don't start with '['
        let summary = "photo.jpg: 1 file pushed, 0 skipped. 12.3 MB/s (3145728 bytes in 0.244s)";
        assert_eq!(parse_progress_line(summary), None);
    }

    #[test]
    fn parse_progress_empty() {
        assert_eq!(parse_progress_line(""), None);
    }

    #[test]
    fn parse_speed_megabytes() {
        let line = "photo.jpg: 1 file pushed, 0 skipped. 12.3 MB/s (3145728 bytes in 0.244s)";
        assert_eq!(parse_speed(line), Some("12.3 MB/s".to_string()));
    }

    #[test]
    fn parse_speed_kilobytes() {
        let line = "file.txt: 1 file pushed, 0 skipped. 512.0 KB/s (512000 bytes in 1.000s)";
        assert_eq!(parse_speed(line), Some("512.0 KB/s".to_string()));
    }

    #[test]
    fn parse_speed_no_paren() {
        // malformed — no parenthesis
        assert_eq!(parse_speed("something without paren"), None);
    }

    // ── run_transfer failure path ─────────────────────────────────────────────

    /// Verify that run_transfer emits TransferEvent::Failed when the adb
    /// subprocess exits with a non-zero status code.
    #[tokio::test]
    #[cfg(unix)]
    async fn run_transfer_emits_failed_on_bad_exit() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::Ordering;

        // Write a tiny script that always exits 1
        let tmp = tempfile::tempdir().expect("tempdir");
        let script = tmp.path().join("fail-adb.sh");
        std::fs::write(&script, b"#!/bin/sh\nexit 1\n").expect("write script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod +x");

        let job = TransferJob {
            id: 99,
            adb_path: script,
            serial: "test".to_string(),
            source: std::path::PathBuf::from("/sdcard/file.jpg"),
            destination: tmp.path().join("file.jpg"),
            direction: TransferDirection::ToLocal,
            filename: "file.jpg".to_string(),
        };

        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let _ = cancel.load(Ordering::Relaxed); // suppress unused warning
        let mut events: Vec<TransferEvent> = Vec::new();

        run_transfer(&job, cancel, |ev| events.push(ev))
            .await
            .expect("run_transfer itself should not error");

        assert!(
            events.iter().any(|e| matches!(e, TransferEvent::Failed(_))),
            "expected a Failed event when adb exits 1, got: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, TransferEvent::Complete { .. })),
            "should not receive Complete when adb exits 1"
        );
    }
}
