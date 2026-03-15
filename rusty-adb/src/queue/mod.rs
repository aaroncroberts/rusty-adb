//! Background file copy queue: domain model, persistence, and manager.
//!
//! # Overview
//!
//! - [`QueueItem`]   — a single local→Android copy job with lifecycle tracking
//! - [`QueueStatus`] — the five states an item can be in
//! - [`QueueManager`] — owns all items, pause/resume flag, and JSON persistence
//!
//! The queue is persisted to `~/.rusty-adb/queue.json` after every mutation
//! so it survives app restarts. On load, items that were mid-copy (`Copying`)
//! are reset to `Pending` because the transfer process terminated.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Queue Status ─────────────────────────────────────────────────────────────

/// Lifecycle state of a single [`QueueItem`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum QueueStatus {
    /// Waiting to be processed.
    Pending,
    /// Actively being transferred — `percent` is 0–100.
    Copying { percent: u8 },
    /// Queue is paused; this item is next in line but not running.
    Paused,
    /// Transfer completed successfully.
    Done,
    /// Transfer failed — `reason` is the human-readable error.
    Failed { reason: String },
}

impl QueueStatus {
    /// Short display label for the queue UI.
    pub fn label(&self) -> &str {
        match self {
            QueueStatus::Pending => "Pending",
            QueueStatus::Copying { .. } => "Copying",
            QueueStatus::Paused => "Paused",
            QueueStatus::Done => "Done",
            QueueStatus::Failed { .. } => "Failed",
        }
    }
}

// ─── Queue Item ───────────────────────────────────────────────────────────────

/// A single enqueued copy job: one local file or folder → an Android destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    /// Unique identifier (monotonically incrementing u64 assigned by [`QueueManager`]).
    pub id: u64,
    /// Absolute path on the local machine to copy from.
    pub local_path: PathBuf,
    /// Absolute path on the Android device to copy into.
    pub android_dest: PathBuf,
    /// Current lifecycle state.
    pub status: QueueStatus,
    /// ISO 8601 timestamp when the item was enqueued (UTC, e.g. `"2026-03-14T10:00:00Z"`).
    pub enqueued_at: String,
}

impl QueueItem {
    /// Display name for the item — last path component of `local_path`.
    pub fn display_name(&self) -> String {
        self.local_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.local_path.display().to_string())
    }
}

// ─── Queue Manager ────────────────────────────────────────────────────────────

/// Owns the queue item collection, pause state, and JSON persistence.
///
/// Calling any mutating method (`enqueue`, `update_status`, `remove`, `set_paused`)
/// automatically attempts to persist the queue to disk via [`QueueManager::save`].
/// Persistence errors are logged as warnings but never propagate — the queue is
/// an enhancement, not a critical path.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct QueueManager {
    /// All items — pending, active, paused, done, and failed.
    pub items: Vec<QueueItem>,
    /// When `true` the copy engine will not dequeue new jobs.
    pub is_paused: bool,
    /// Next id to assign (auto-increments).
    #[serde(default)]
    next_id: u64,
    /// Path where queue is persisted (not serialized — resolved at runtime).
    #[serde(skip)]
    pub persist_path: Option<PathBuf>,
}

impl QueueManager {
    /// Create an empty manager that persists to `path`.
    #[allow(dead_code)] // used by queue unit tests
    pub fn new(persist_path: PathBuf) -> Self {
        Self {
            persist_path: Some(persist_path),
            ..Default::default()
        }
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    /// Load from `~/.rusty-adb/queue.json`.
    /// Returns an empty manager if the file is absent or unreadable.
    /// Items that were mid-copy (`Copying`) are reset to `Pending`.
    pub fn load(path: PathBuf) -> Self {
        let mut mgr: QueueManager = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        mgr.persist_path = Some(path);

        // Resumable: any item interrupted mid-copy reverts to Pending
        for item in &mut mgr.items {
            if matches!(item.status, QueueStatus::Copying { .. }) {
                tracing::info!(
                    id = item.id,
                    name = %item.display_name(),
                    "queue: resetting interrupted copy to Pending on load"
                );
                item.status = QueueStatus::Pending;
            }
        }

        mgr
    }

    /// Write the current queue to disk (non-fatal on error).
    ///
    /// Uses an atomic write (temp file → rename) to avoid corrupt reads if the
    /// app is killed mid-write.  The `~/.rusty-adb/` directory is created if
    /// absent — it was created at startup but this is a safe no-op if it exists.
    pub fn save(&self) {
        let Some(ref path) = self.persist_path else {
            return;
        };
        let json = match serde_json::to_string_pretty(self) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!(error = %e, "queue: failed to serialize queue");
                return;
            }
        };
        // Ensure parent directory exists
        if let Some(dir) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!(error = %e, dir = %dir.display(), "queue: could not create app dir");
            }
        }
        // Atomic write: write to a sibling temp file then rename into place
        let tmp = path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, &json) {
            tracing::warn!(error = %e, path = %tmp.display(), "queue: failed to write temp file");
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            tracing::warn!(error = %e, "queue: atomic rename failed, removing temp file");
            let _ = std::fs::remove_file(&tmp);
        }
    }

    // ── Mutations ─────────────────────────────────────────────────────────────

    /// Add a new item to the back of the queue, then persist.
    /// Returns the id assigned to the new item.
    pub fn enqueue(&mut self, local_path: PathBuf, android_dest: PathBuf) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let enqueued_at = utc_now();
        tracing::info!(
            id,
            local = %local_path.display(),
            dest  = %android_dest.display(),
            "queue: item enqueued"
        );
        self.items.push(QueueItem {
            id,
            local_path,
            android_dest,
            status: QueueStatus::Pending,
            enqueued_at,
        });
        self.save();
        id
    }

    /// Update the status of an item by `id`. No-op if `id` is not found.
    pub fn update_status(&mut self, id: u64, status: QueueStatus) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            tracing::debug!(
                id,
                name = %item.display_name(),
                new_status = status.label(),
                "queue: status updated"
            );
            item.status = status;
            self.save();
        }
    }

    /// Update the android destination of an item by `id`. No-op if not found.
    pub fn update_dest(&mut self, id: u64, android_dest: PathBuf) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            tracing::info!(
                id,
                name = %item.display_name(),
                new_dest = %android_dest.display(),
                "queue: destination updated"
            );
            item.android_dest = android_dest;
            // Re-pend if it had previously failed so it can be retried
            if matches!(item.status, QueueStatus::Failed { .. }) {
                item.status = QueueStatus::Pending;
            }
            self.save();
        }
    }

    /// Remove an item by `id`. Returns the removed item, or `None` if not found.
    pub fn remove(&mut self, id: u64) -> Option<QueueItem> {
        if let Some(pos) = self.items.iter().position(|i| i.id == id) {
            let removed = self.items.remove(pos);
            tracing::info!(id, name = %removed.display_name(), "queue: item removed");
            self.save();
            Some(removed)
        } else {
            None
        }
    }

    /// Remove all items with `Done` or `Failed` status, then persist.
    pub fn clear_done(&mut self) {
        let before = self.items.len();
        self.items
            .retain(|i| !matches!(i.status, QueueStatus::Done | QueueStatus::Failed { .. }));
        let removed = before - self.items.len();
        if removed > 0 {
            tracing::info!(removed, "queue: cleared completed/failed items");
            self.save();
        }
    }

    /// Set pause state and persist.
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
        tracing::info!(paused, "queue: paused state changed");
        self.save();
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    /// Next item eligible to be copied: first `Pending` item, if not paused.
    pub fn next_pending(&self) -> Option<&QueueItem> {
        if self.is_paused {
            return None;
        }
        self.items.iter().find(|i| i.status == QueueStatus::Pending)
    }

    /// Count of items in each category, for the status bar summary.
    pub fn summary(&self) -> QueueSummary {
        let mut pending = 0u32;
        let mut copying = 0u32;
        let mut done = 0u32;
        let mut failed = 0u32;
        let mut active_percent = None;

        for item in &self.items {
            match &item.status {
                QueueStatus::Pending | QueueStatus::Paused => pending += 1,
                QueueStatus::Copying { percent } => {
                    copying += 1;
                    active_percent = Some(*percent);
                }
                QueueStatus::Done => done += 1,
                QueueStatus::Failed { .. } => failed += 1,
            }
        }
        QueueSummary {
            pending,
            copying,
            done,
            failed,
            active_percent,
            is_paused: self.is_paused,
        }
    }
}

// ─── Queue Summary ────────────────────────────────────────────────────────────

/// Snapshot counts for the status bar display.
#[derive(Debug, Clone, Default)]
pub struct QueueSummary {
    pub pending: u32,
    pub copying: u32,
    pub done: u32,
    pub failed: u32,
    /// Progress of the actively copying item (0–100), if any.
    pub active_percent: Option<u8>,
    pub is_paused: bool,
}

impl QueueSummary {
    /// `true` when there are items that need attention (not all done/empty).
    pub fn has_activity(&self) -> bool {
        self.pending > 0 || self.copying > 0 || self.failed > 0
    }

    /// Short status bar text, e.g. `"Copying 1/3 · 42%"` or `"3 pending"`.
    pub fn status_text(&self) -> String {
        if self.copying > 0 {
            let total = self.pending + self.copying + self.done;
            let done_idx = self.done + 1;
            if let Some(pct) = self.active_percent {
                return format!("Copying {done_idx}/{total} · {pct}%");
            }
            return format!("Copying {done_idx}/{total}");
        }
        if self.is_paused && self.pending > 0 {
            return format!("Queue paused · {} pending", self.pending);
        }
        if self.pending > 0 {
            return format!("{} pending", self.pending);
        }
        if self.failed > 0 {
            return format!("{} failed", self.failed);
        }
        String::new()
    }
}

// ─── UTC timestamp helper ─────────────────────────────────────────────────────

fn utc_now() -> String {
    // Use std::time for a zero-dependency UTC timestamp in ISO 8601 format.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, m, d) = crate::fs::days_to_ymd((secs / 86400) as i64);
    let rem = secs % 86400;
    let h = rem / 3600;
    let min = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn mgr() -> QueueManager {
        QueueManager::default()
    }

    #[test]
    fn enqueue_assigns_incrementing_ids() {
        let mut q = mgr();
        let id0 = q.enqueue(p("/home/a.jpg"), p("/sdcard/Pictures"));
        let id1 = q.enqueue(p("/home/b.jpg"), p("/sdcard/Pictures"));
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(q.items.len(), 2);
    }

    #[test]
    fn next_pending_returns_first_pending() {
        let mut q = mgr();
        q.enqueue(p("/a"), p("/d"));
        q.enqueue(p("/b"), p("/d"));
        let first = q.next_pending().unwrap();
        assert_eq!(first.id, 0);
    }

    #[test]
    fn next_pending_is_none_when_paused() {
        let mut q = mgr();
        q.enqueue(p("/a"), p("/d"));
        q.set_paused(true);
        assert!(q.next_pending().is_none());
    }

    #[test]
    fn remove_extracts_item() {
        let mut q = mgr();
        let id = q.enqueue(p("/a"), p("/d"));
        let removed = q.remove(id).unwrap();
        assert_eq!(removed.id, id);
        assert!(q.items.is_empty());
    }

    #[test]
    fn update_status_changes_item() {
        let mut q = mgr();
        let id = q.enqueue(p("/a"), p("/d"));
        q.update_status(id, QueueStatus::Copying { percent: 50 });
        assert_eq!(q.items[0].status, QueueStatus::Copying { percent: 50 });
    }

    #[test]
    fn summary_counts_correctly() {
        let mut q = mgr();
        let id0 = q.enqueue(p("/a"), p("/d"));
        let id1 = q.enqueue(p("/b"), p("/d"));
        q.enqueue(p("/c"), p("/d")); // pending
        q.update_status(id0, QueueStatus::Done);
        q.update_status(id1, QueueStatus::Copying { percent: 42 });

        let s = q.summary();
        assert_eq!(s.done, 1);
        assert_eq!(s.copying, 1);
        assert_eq!(s.pending, 1);
        assert_eq!(s.active_percent, Some(42));
    }

    #[test]
    fn summary_status_text_while_copying() {
        let mut q = mgr();
        let id = q.enqueue(p("/a"), p("/d"));
        q.enqueue(p("/b"), p("/d"));
        q.update_status(id, QueueStatus::Copying { percent: 75 });
        assert_eq!(q.summary().status_text(), "Copying 1/2 · 75%");
    }

    #[test]
    fn update_dest_re_pends_failed_item() {
        let mut q = mgr();
        let id = q.enqueue(p("/a"), p("/d"));
        q.update_status(
            id,
            QueueStatus::Failed {
                reason: "error".into(),
            },
        );
        q.update_dest(id, p("/d2"));
        assert_eq!(q.items[0].status, QueueStatus::Pending);
        assert_eq!(q.items[0].android_dest, p("/d2"));
    }

    #[test]
    fn load_resets_copying_items_to_pending() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.json");

        let mut q = QueueManager::new(path.clone());
        let id = q.enqueue(p("/a"), p("/d"));
        q.update_status(id, QueueStatus::Copying { percent: 50 });
        q.save();

        let loaded = QueueManager::load(path);
        assert_eq!(loaded.items[0].status, QueueStatus::Pending);
    }

    #[test]
    fn persist_and_reload_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.json");

        let mut q = QueueManager::new(path.clone());
        q.enqueue(p("/local/photo.jpg"), p("/storage/emulated/0/DCIM"));
        q.set_paused(true);
        q.save();

        let loaded = QueueManager::load(path);
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].local_path, p("/local/photo.jpg"));
        assert!(loaded.is_paused);
    }
}
