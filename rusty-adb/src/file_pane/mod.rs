//! Unified file browser pane — [`FilePane<FS>`].
//!
//! A single generic struct that replaces both `LocalPane` and `AndroidPane`.
//! The backend is swapped via the [`FileSystem`] type parameter:
//!
//! - `FilePane<LocalFs>` — browses the host filesystem
//! - `FilePane<AndroidFs>` — browses an Android device via ADB
//!
//! # State machine
//!
//! ```text
//!  (LocalFs)  Ready ──(navigate)──► Loading ──► Ready
//!                                        └──► Error
//!
//!  (AndroidFs) NoDevice ──(device connected)──► Loading ──► Ready
//!                                                   └──► Error ──(retry)──► Loading
//! ```
//!
//! The local pane never enters `NoDevice` or stays in `Loading` longer than a
//! single async round-trip; the transition is imperceptible to the user.

mod list_view;
mod shared_views;

pub use shared_views::view_breadcrumb;
#[allow(unused_imports)]
pub use shared_views::{view_connect_guide, view_error};

use std::path::PathBuf;

use crate::fs::{DirEntry, FileSystem, PaneState, SortField, sort_entries};

// ─── Rename State ─────────────────────────────────────────────────────────────

/// Stable widget ID for the inline rename text input — used by `text_input::focus`.
pub const RENAME_INPUT_ID: &str = "pane-rename-input";

// ─── File Pane ────────────────────────────────────────────────────────────────

/// A generic file browser pane backed by `FS`.
///
/// `FilePane<LocalFs>` and `FilePane<AndroidFs>` are two independent
/// monomorphized types; the compiler generates separate, specialized code
/// for each with zero overhead.
#[derive(Debug, Clone)]
pub struct FilePane<FS: FileSystem> {
    // ── Navigation ────────────────────────────────────────────────────────
    pub current_path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected: Vec<usize>,

    // ── Lifecycle ─────────────────────────────────────────────────────────
    pub state: PaneState,

    // ── Display options ───────────────────────────────────────────────────
    pub show_hidden: bool,
    pub sort_by: SortField,
    pub sort_ascending: bool,
    pub show_type: bool,
    pub show_size: bool,
    pub show_modified: bool,

    // ── Android-specific extras (zero-cost for LocalFs) ───────────────────
    /// Storage roots that block upward navigation (e.g. `/sdcard`).
    /// Always empty for `FilePane<LocalFs>`.
    pub storage_roots: Vec<PathBuf>,

    /// Spinner frame counter driven by `SpinnerTick` messages (Android only).
    pub spinner_frame: u8,

    /// Active inline rename: `(entry_index, draft_text)`.
    /// `None` when no rename is in progress.
    pub rename_pending: Option<(usize, String)>,

    // ── Phantom ───────────────────────────────────────────────────────────
    _fs: std::marker::PhantomData<FS>,
}

// ─── Construction ─────────────────────────────────────────────────────────────

impl<FS: FileSystem> FilePane<FS> {
    /// Create a new pane rooted at `path` in the given initial state.
    ///
    /// - Local pane: pass `PaneState::Loading` — the caller immediately
    ///   dispatches `LocalNavigateTo` which kicks off the async load.
    /// - Android pane: pass `PaneState::NoDevice` — loading begins when a
    ///   device connects.
    pub fn new(path: PathBuf, initial_state: PaneState) -> Self {
        Self {
            current_path: path,
            entries: Vec::new(),
            selected: Vec::new(),
            state: initial_state,
            show_hidden: false,
            sort_by: SortField::Name,
            sort_ascending: true,
            show_type: true,
            show_size: true,
            show_modified: true,
            storage_roots: Vec::new(),
            spinner_frame: 0,
            rename_pending: None,
            _fs: std::marker::PhantomData,
        }
    }
}

// ─── State Transitions ────────────────────────────────────────────────────────

impl<FS: FileSystem> FilePane<FS> {
    /// Begin navigating to `path`: update `current_path`, clear selection,
    /// and transition to `Loading`.
    pub fn begin_navigate(&mut self, path: PathBuf) {
        tracing::info!(
            from = %self.current_path.display(),
            to   = %path.display(),
            "file pane: navigating"
        );
        self.current_path = path;
        self.selected.clear();
        self.rename_pending = None;
        self.state = PaneState::Loading;
    }

    /// Called when the async `list_dir` task completes successfully.
    ///
    /// Stale responses (where `path` no longer matches `current_path`) are
    /// silently discarded — this prevents a race where the user navigates
    /// away before a slow ADB listing arrives.
    pub fn on_entries_loaded(&mut self, path: PathBuf, mut entries: Vec<DirEntry>) {
        if self.current_path != path {
            tracing::debug!(
                expected = %self.current_path.display(),
                arrived  = %path.display(),
                "file pane: discarding stale directory response"
            );
            return;
        }
        sort_entries(&mut entries, self.sort_by, self.sort_ascending);
        self.entries = entries;
        self.state = PaneState::Ready;
        tracing::info!(
            path  = %path.display(),
            count = self.entries.len(),
            "file pane: directory loaded"
        );
    }

    /// Called when the async `list_dir` task returns an error.
    pub fn on_error(&mut self, msg: String) {
        tracing::warn!(
            path  = %self.current_path.display(),
            error = %msg,
            "file pane: directory load error"
        );
        self.state = PaneState::Error(msg);
    }

    /// Toggle selection of entry `index` (multi-select toggle).
    pub fn select(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        if let Some(pos) = self.selected.iter().position(|&i| i == index) {
            self.selected.remove(pos);
        } else {
            self.selected.push(index);
        }
    }

    /// Toggle hidden-file visibility (filter applied at render time).
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
    }

    /// Toggle Type column visibility.
    pub fn toggle_type(&mut self) {
        self.show_type = !self.show_type;
    }

    /// Toggle Size column visibility.
    pub fn toggle_size(&mut self) {
        self.show_size = !self.show_size;
    }

    /// Toggle Modified column visibility.
    pub fn toggle_modified(&mut self) {
        self.show_modified = !self.show_modified;
    }

    /// Change sort field (or toggle direction if the same field is clicked again).
    /// Re-sorts the current entries in-place.
    pub fn set_sort(&mut self, field: SortField) {
        if self.sort_by == field {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_by = field;
            self.sort_ascending = true;
        }
        sort_entries(&mut self.entries, self.sort_by, self.sort_ascending);
    }

    /// Advance the spinner animation by one frame (Android loading state only).
    pub fn tick_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }

    // ── Rename helpers ────────────────────────────────────────────────────

    /// Begin an inline rename for entry `index`, pre-filling the input with
    /// the current name.
    pub fn begin_rename(&mut self, index: usize) {
        if let Some(entry) = self.entries.get(index) {
            self.rename_pending = Some((index, entry.name.clone()));
        }
    }

    /// Update the draft text for the active rename.
    pub fn update_rename_input(&mut self, new_value: String) {
        if let Some((_, ref mut text)) = self.rename_pending {
            *text = new_value;
        }
    }

    /// Cancel the active rename without committing.
    pub fn cancel_rename(&mut self) {
        self.rename_pending = None;
    }
}

// ─── Rename Callbacks ─────────────────────────────────────────────────────────

/// Callbacks passed to [`FilePane::view_list`] to enable inline rename.
///
/// Pass `None` for backends that do not support rename (local filesystem).
pub struct RenameCbs<'a, Message> {
    pub on_input: Box<dyn Fn(String) -> Message + 'a>,
    pub on_commit: Message,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{LocalFs, PaneState};

    fn make_pane() -> FilePane<LocalFs> {
        FilePane::new(PathBuf::from("/sdcard"), PaneState::NoDevice)
    }

    #[test]
    fn new_pane_defaults() {
        let pane = make_pane();
        assert_eq!(pane.current_path, PathBuf::from("/sdcard"));
        assert_eq!(pane.state, PaneState::NoDevice);
        assert!(pane.entries.is_empty());
        assert!(pane.selected.is_empty());
        assert!(!pane.show_hidden);
        assert_eq!(pane.sort_by, SortField::Name);
        assert!(pane.sort_ascending);
    }

    #[test]
    fn begin_navigate_sets_loading() {
        let mut pane = make_pane();
        pane.begin_navigate(PathBuf::from("/sdcard/DCIM"));
        assert_eq!(pane.state, PaneState::Loading);
        assert_eq!(pane.current_path, PathBuf::from("/sdcard/DCIM"));
        assert!(pane.selected.is_empty());
    }

    #[test]
    fn on_entries_loaded_transitions_to_ready() {
        let path = PathBuf::from("/sdcard");
        let mut pane = make_pane();
        pane.begin_navigate(path.clone());
        pane.on_entries_loaded(path.clone(), vec![]);
        assert_eq!(pane.state, PaneState::Ready);
    }

    #[test]
    fn on_entries_loaded_stale_is_ignored() {
        let path = PathBuf::from("/sdcard");
        let mut pane = make_pane();
        pane.begin_navigate(path.clone());
        // Navigate away before entries arrive
        pane.begin_navigate(PathBuf::from("/sdcard/DCIM"));
        // Stale response for the old path
        pane.on_entries_loaded(path, vec![]);
        // Should still be Loading (waiting for DCIM response), not Ready
        assert_eq!(pane.state, PaneState::Loading);
    }

    #[test]
    fn on_error_transitions_to_error() {
        let mut pane = make_pane();
        pane.begin_navigate(PathBuf::from("/sdcard/private"));
        pane.on_error("Permission denied".to_string());
        assert!(matches!(pane.state, PaneState::Error(_)));
    }

    #[test]
    fn select_toggles_entry() {
        let path = PathBuf::from("/tmp");
        let mut pane: FilePane<LocalFs> = FilePane::new(path.clone(), PaneState::Loading);
        pane.on_entries_loaded(path, vec![
            DirEntry {
                name: "a".into(),
                path: PathBuf::from("/tmp/a"),
                size: 0,
                modified_display: "--".into(),
                is_dir: false,
                is_symlink: false,
                is_hidden: false,
            },
        ]);
        pane.select(0);
        assert!(pane.selected.contains(&0));
        pane.select(0);
        assert!(!pane.selected.contains(&0));
    }

    #[test]
    fn select_out_of_bounds_is_ignored() {
        let mut pane = make_pane();
        pane.select(usize::MAX);
        assert!(pane.selected.is_empty());
    }

    #[test]
    fn toggle_hidden_flips_flag() {
        let mut pane = make_pane();
        assert!(!pane.show_hidden);
        pane.toggle_hidden();
        assert!(pane.show_hidden);
        pane.toggle_hidden();
        assert!(!pane.show_hidden);
    }

    #[test]
    fn set_sort_same_field_toggles_direction() {
        let mut pane = make_pane();
        assert_eq!(pane.sort_by, SortField::Name);
        assert!(pane.sort_ascending);
        pane.set_sort(SortField::Name);
        assert!(!pane.sort_ascending);
        pane.set_sort(SortField::Name);
        assert!(pane.sort_ascending);
    }

    #[test]
    fn set_sort_new_field_resets_ascending() {
        let mut pane = make_pane();
        pane.set_sort(SortField::Name); // now descending
        pane.set_sort(SortField::Size); // new field → ascending
        assert_eq!(pane.sort_by, SortField::Size);
        assert!(pane.sort_ascending);
    }

    #[test]
    fn begin_rename_sets_pending() {
        let path = PathBuf::from("/tmp");
        let mut pane: FilePane<LocalFs> = FilePane::new(path.clone(), PaneState::Loading);
        pane.on_entries_loaded(path, vec![
            DirEntry {
                name: "hello.txt".into(),
                path: PathBuf::from("/tmp/hello.txt"),
                size: 0,
                modified_display: "--".into(),
                is_dir: false,
                is_symlink: false,
                is_hidden: false,
            },
        ]);
        pane.begin_rename(0);
        assert_eq!(pane.rename_pending, Some((0, "hello.txt".to_string())));
    }

    #[test]
    fn cancel_rename_clears_pending() {
        let mut pane = make_pane();
        pane.rename_pending = Some((0, "old".to_string()));
        pane.cancel_rename();
        assert!(pane.rename_pending.is_none());
    }
}
