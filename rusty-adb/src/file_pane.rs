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

use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Border, Color, Element, Fill};

use crate::filesystem::{DirEntry, FileSystem, PaneState, SortField};
use crate::local_fs::sort_entries;
use crate::theme::ThemeColors;

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

// ─── View ─────────────────────────────────────────────────────────────────────

impl<FS: FileSystem> FilePane<FS> {
    /// Render the pane in **List** mode: sticky sort header + scrollable entry rows.
    ///
    /// # Parameters
    /// - `ctx`         — backend context (used for `is_nav_root`)
    /// - `theme`       — colour palette
    /// - `on_navigate` — message to emit when the user clicks a directory
    /// - `on_select`   — message to emit when the user clicks a file
    /// - `on_sort`     — message to emit when a column header is clicked
    /// - `rename_cbs`  — optional rename callbacks; `None` for backends that
    ///                   do not support inline rename (local filesystem)
    pub fn view_list<'a, 'c, Message: 'a + Clone>(
        &'a self,
        ctx: &'c FS::Context,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_sort: impl Fn(SortField) -> Message + 'a,
        rename_cbs: Option<RenameCbs<'a, Message>>,
    ) -> Element<'a, Message> {
        // ── Column widths (shared between header and every data row) ──────────
        const W_ICON: u16 = 24;
        const W_TYPE: u16 = 52;
        const W_SIZE: u16 = 72;
        const W_DATE: u16 = 90;

        let t = theme;

        // ── Sort indicator ────────────────────────────────────────────────────
        let sort_ind = |field: SortField| -> &'static str {
            if self.sort_by == field {
                if self.sort_ascending { " ▲" } else { " ▼" }
            } else {
                ""
            }
        };

        // ── Column header builder ─────────────────────────────────────────────
        let mk_hdr = |label: String, field: SortField, width: iced::Length, msg: Message|
            -> Element<'a, Message>
        {
            let fg = if self.sort_by == field { t.accent } else { t.text_secondary };
            button(text(label).size(10).color(fg))
                .width(width)
                .padding([2, 4])
                .style(move |_t, _s| button::Style { background: None, ..Default::default() })
                .on_press(msg)
                .into()
        };

        // ── Sticky column header row ──────────────────────────────────────────
        let name_lbl = format!("Name{}", sort_ind(SortField::Name));
        let size_lbl = format!("Size{}", sort_ind(SortField::Size));
        let date_lbl = format!("Modified{}", sort_ind(SortField::Modified));

        let mut hdr_cells: Vec<Element<Message>> = vec![
            iced::widget::Space::new(20, 1).into(), // checkbox placeholder
            text("").width(W_ICON).into(),           // icon placeholder
            mk_hdr(name_lbl, SortField::Name, Fill, on_sort(SortField::Name)),
        ];
        if self.show_type {
            hdr_cells.push(mk_hdr(
                "Type".to_string(),
                SortField::Name,
                iced::Length::Fixed(W_TYPE as f32),
                on_sort(SortField::Name),
            ));
        }
        if self.show_size {
            hdr_cells.push(mk_hdr(
                size_lbl,
                SortField::Size,
                iced::Length::Fixed(W_SIZE as f32),
                on_sort(SortField::Size),
            ));
        }
        if self.show_modified {
            hdr_cells.push(mk_hdr(
                date_lbl,
                SortField::Modified,
                iced::Length::Fixed(W_DATE as f32),
                on_sort(SortField::Modified),
            ));
        }

        let col_header = container(
            iced::widget::Row::from_vec(hdr_cells)
                .width(Fill)
                .spacing(6)
                .padding([2, 8])
                .align_y(iced::Alignment::Center),
        )
        .width(Fill)
        .style(move |_t| container::Style {
            background: Some(t.background_secondary.into()),
            border: Border { color: t.border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        });

        // ── State guard: show spinner / error / no-device ────────────────────
        if self.state != PaneState::Ready {
            let body = self.view_state_body(theme);
            return column![col_header, body].width(Fill).height(Fill).into();
        }

        // ── Entry rows ────────────────────────────────────────────────────────
        let is_at_nav_root = FS::is_nav_root(ctx, &self.current_path);

        // Pre-build the inline rename input (consumes rename callbacks once).
        let mut rename_row: Option<(usize, Element<Message>)> =
            if let (Some(cbs), Some((idx, val))) = (rename_cbs, &self.rename_pending) {
                let icon = self
                    .entries
                    .get(*idx)
                    .map(|e| e.icon())
                    .unwrap_or("[-]");
                let input = text_input("New name…", val.as_str())
                    .id(text_input::Id::new(RENAME_INPUT_ID))
                    .on_input(cbs.on_input)
                    .on_submit(cbs.on_commit)
                    .size(12)
                    .width(Fill);
                Some((*idx, row![text(icon).size(12), input].spacing(6).padding([1, 0]).into()))
            } else {
                None
            };

        let mut rows: Vec<Element<Message>> = Vec::new();

        // ".." up-navigation
        if !is_at_nav_root {
            if let Some(parent) = self.current_path.parent() {
                let parent_path = parent.to_path_buf();
                rows.push(
                    button(
                        row![
                            text("[/]").size(11).color(t.accent).width(W_ICON),
                            text("..").size(12).color(t.text),
                            iced::widget::Space::with_width(Fill),
                        ]
                        .spacing(6)
                        .padding([1, 0]),
                    )
                    .width(Fill)
                    .style(move |_t, _s| button::Style { background: None, ..Default::default() })
                    .on_press(on_navigate(parent_path))
                    .into(),
                );
            }
        }

        // Visible entry count (hidden filter applied at render time)
        let visible_count = if self.show_hidden {
            self.entries.len()
        } else {
            self.entries.iter().filter(|e| !e.is_hidden).count()
        };

        if visible_count == 0 {
            let msg = if self.entries.is_empty() {
                "This directory is empty"
            } else {
                "All entries are hidden  (Show → toggle hidden)"
            };
            rows.push(
                container(text(msg).size(11).color(t.text_secondary))
                    .padding([8, 12])
                    .into(),
            );
        }

        for (i, entry) in self.entries.iter().enumerate() {
            if !self.show_hidden && entry.is_hidden {
                continue;
            }

            let is_selected = self.selected.contains(&i);
            let bg: Option<iced::Background> =
                if is_selected { Some(t.accent.scale_alpha(0.2).into()) } else { None };
            let fg = if entry.is_hidden { t.text_secondary } else { t.text };
            let icon_fg = if entry.is_navigable() { t.accent } else { t.text_secondary };

            let entry_path = entry.path.clone();
            let can_navigate = entry.is_navigable();
            let on_nav = on_navigate(entry_path);
            let on_sel_btn = on_select(i);
            let on_sel_chk = on_select(i);

            // If this entry is being renamed, use the pre-built rename input.
            let row_element: Element<Message> =
                if rename_row.as_ref().map(|(idx, _)| *idx == i).unwrap_or(false) {
                    rename_row.take().unwrap().1
                } else {
                    let mut cells: Vec<Element<Message>> = vec![
                        text(entry.icon()).size(11).color(icon_fg).width(W_ICON).into(),
                        text(entry.name.clone()).size(11).color(fg).width(Fill).into(),
                    ];
                    if self.show_type {
                        cells.push(
                            text(entry.type_label())
                                .size(10)
                                .color(t.text_secondary)
                                .width(W_TYPE)
                                .into(),
                        );
                    }
                    if self.show_size {
                        cells.push(
                            text(entry.size_display())
                                .size(10)
                                .color(t.text_secondary)
                                .width(W_SIZE)
                                .into(),
                        );
                    }
                    if self.show_modified {
                        cells.push(
                            text(entry.modified_display.clone())
                                .size(10)
                                .color(t.text_secondary)
                                .width(W_DATE)
                                .into(),
                        );
                    }

                    container(
                        row![
                            checkbox("", is_selected)
                                .on_toggle(move |_| on_sel_chk.clone())
                                .size(12),
                            button(
                                iced::widget::Row::from_vec(cells)
                                    .width(Fill)
                                    .spacing(6)
                                    .align_y(iced::Alignment::Center),
                            )
                            .width(Fill)
                            .style(|_t, _s| button::Style { background: None, ..Default::default() })
                            .on_press(if can_navigate { on_nav } else { on_sel_btn }),
                        ]
                        .spacing(4)
                        .padding([2, 8])
                        .align_y(iced::Alignment::Center),
                    )
                    .width(Fill)
                    .style(move |_t| container::Style {
                        background: bg,
                        ..Default::default()
                    })
                    .into()
                };

            rows.push(row_element);
        }

        let body = container(
            scrollable(column(rows).width(Fill))
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(move |_t| container::Style {
            background: Some(t.background.into()),
            ..Default::default()
        });

        column![col_header, body].width(Fill).height(Fill).into()
    }

    // ── State body helpers ────────────────────────────────────────────────────

    fn view_state_body<'a, Message: 'a + Clone>(&'a self, theme: ThemeColors) -> Element<'a, Message> {
        match &self.state {
            PaneState::NoDevice => view_connect_guide(theme),
            PaneState::Loading => self.view_loading(theme),
            PaneState::Error(e) => view_error(theme, e),
            PaneState::Ready => unreachable!("guarded above"),
        }
    }

    fn view_loading<'a, Message: 'a + Clone>(&'a self, theme: ThemeColors) -> Element<'a, Message> {
        let spinner_chars = ["|", "/", "-", "\\"];
        let spinner = spinner_chars[(self.spinner_frame as usize) % spinner_chars.len()];

        let content = row![
            text(spinner).size(14).color(theme.accent),
            text("  Fetching directory listing…").size(12).color(theme.text_secondary),
        ]
        .align_y(iced::Alignment::Center);

        container(content)
            .width(Fill)
            .height(Fill)
            .padding(24)
            .style(move |_t| container::Style {
                background: Some(theme.background.into()),
                ..Default::default()
            })
            .into()
    }
}

// ─── Free View Helpers ────────────────────────────────────────────────────────

/// Error state body — shown when `list_dir` fails.
pub fn view_error<'a, Message: 'a + Clone>(theme: ThemeColors, msg: &str) -> Element<'a, Message> {
    let content = column![
        text("Error").size(13).color(theme.text_secondary),
        text(msg.to_string()).size(11).color(theme.error),
    ]
    .spacing(6);

    container(content)
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        })
        .into()
}

/// No-device guide — shown when no Android device is connected.
///
/// Moved here from `android_pane.rs` since it has no backend-specific logic.
pub fn view_connect_guide<'a, Message: 'a + Clone>(theme: ThemeColors) -> Element<'a, Message> {
    let guide = column![
        text("No Android device connected").size(13).color(theme.text_secondary),
        text("").size(6),
        text("1. Enable USB Debugging on your device").size(11).color(theme.text_secondary),
        text("2. Connect via USB").size(11).color(theme.text_secondary),
        text("3. Tap  Allow  when prompted").size(11).color(theme.text_secondary),
    ]
    .spacing(4);

    container(guide)
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        })
        .into()
}

// ─── Breadcrumb ───────────────────────────────────────────────────────────────

/// Render a clickable breadcrumb bar for `current_path`.
///
/// Produces a row like:  `/` › `Users` › `aaron` › `Documents`
///
/// Each segment except the last emits `on_navigate(ancestor_path)` when clicked.
/// Messages are computed eagerly so the element lifetime is independent of the caller.
pub fn view_breadcrumb<'a, Message: 'a + Clone>(
    current_path: &std::path::Path,
    theme: ThemeColors,
    on_navigate: impl Fn(PathBuf) -> Message,
) -> iced::widget::Container<'a, Message> {
    use iced::{Border, Fill};
    use iced::widget::{button, container, row, text};

    let mut segments: Vec<(String, PathBuf)> = current_path
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| {
            let label = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".to_string());
            (label, p.to_path_buf())
        })
        .collect();

    if segments.first().map(|(l, _)| l.as_str()) != Some("/") {
        segments.insert(0, ("/".to_string(), PathBuf::from("/")));
    }

    let last_idx = segments.len().saturating_sub(1);
    let mut crumb_row: Vec<iced::Element<'a, Message>> = Vec::new();

    for (i, (label, path)) in segments.into_iter().enumerate() {
        let is_last = i == last_idx;
        if is_last {
            crumb_row.push(text(label).size(11).color(theme.accent).into());
        } else {
            let nav_msg = on_navigate(path);
            let btn = button(text(label).size(11).color(theme.text_secondary))
                .style(move |_t, _s| button::Style { background: None, ..Default::default() })
                .padding([0, 2])
                .on_press(nav_msg);
            crumb_row.push(btn.into());
            crumb_row.push(text("›").size(11).color(theme.text_secondary).into());
        }
    }

    let content = row(crumb_row)
        .align_y(iced::Alignment::Center)
        .spacing(2)
        .padding([2, 8]);

    container(content)
        .width(Fill)
        .height(22.0)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            border: Border { color: theme.border, width: 1.0, ..Default::default() },
            ..Default::default()
        })
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
    use crate::filesystem::PaneState;
    use crate::local_fs::LocalFs;

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
