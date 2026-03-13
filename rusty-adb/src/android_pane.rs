//! Android device file browser pane
//!
//! Renders the right pane: directory listing of an Android device via ADB.
//!
//! # State machine
//! ```text
//! NoDevice ──(device connected)──► Loading ──(entries arrive)──► Browsing
//!                                     │                              │
//!                                     └──(adb error)──► Error        │
//!                                                                    ▼
//!                                                           (click directory)
//!                                                                    │
//!                                                                 Loading
//! ```

use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Border, Color, Element, Fill};

use crate::adb::AndroidEntry;
use crate::local_pane::SortField;
use crate::theme::ThemeColors;

// ─── Pane State ───────────────────────────────────────────────────────────────

/// What is the Android pane currently showing?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AndroidPaneState {
    /// No authorized device connected
    NoDevice,
    /// Fetching directory listing from the device (non-blocking)
    Loading,
    /// Directory listing is ready
    Browsing,
    /// ADB returned an error for the last request
    Error(String),
}

/// Stable widget ID for the inline rename text input — used by `text_input::focus`.
pub const RENAME_INPUT_ID: &str = "android-rename-input";

/// State for the Android device file browser pane
#[derive(Debug, Clone)]
pub struct AndroidPane {
    pub state: AndroidPaneState,
    /// Current path on the device (starts at `/sdcard`)
    pub current_path: PathBuf,
    /// Entries returned by the last successful `adb shell ls -la`
    pub entries: Vec<AndroidEntry>,
    /// Selected entry indices (multi-select)
    pub selected: Vec<usize>,
    /// Storage roots discovered on the connected device
    pub storage_roots: Vec<PathBuf>,
    /// Spinner frame counter (incremented by SpinnerTick messages)
    pub spinner_frame: u8,
    /// Active inline rename: (entry_index, current_text_input_value)
    pub rename_pending: Option<(usize, String)>,
    /// Whether to show hidden files (entries starting with '.')
    pub show_hidden: bool,
    /// Active sort field
    pub sort_by: SortField,
    /// True = ascending (A→Z, small→large); false = descending
    pub sort_ascending: bool,
}

impl Default for AndroidPane {
    fn default() -> Self {
        Self {
            state: AndroidPaneState::NoDevice,
            current_path: PathBuf::from("/sdcard"),
            entries: Vec::new(),
            selected: Vec::new(),
            storage_roots: Vec::new(),
            spinner_frame: 0,
            rename_pending: None,
            show_hidden: false,
            sort_by: SortField::Name,
            sort_ascending: true,
        }
    }
}

impl AndroidPane {
    /// Toggle visibility of hidden files (entries whose name starts with '.').
    ///
    /// The filtered view is applied at render time so no ADB re-fetch is needed.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
    }

    /// Called when a device connects — begin loading /sdcard.
    pub fn on_device_connected(&mut self) {
        tracing::info!("android pane: device connected, transitioning to Loading");
        self.current_path = PathBuf::from("/sdcard");
        self.entries.clear();
        self.selected.clear();
        self.state = AndroidPaneState::Loading;
    }

    /// Called when the device disconnects.
    pub fn on_device_disconnected(&mut self) {
        tracing::info!("android pane: device disconnected, transitioning to NoDevice");
        self.entries.clear();
        self.selected.clear();
        self.storage_roots.clear();
        self.state = AndroidPaneState::NoDevice;
    }

    /// Called to start navigating to a new path.
    pub fn begin_navigate(&mut self, path: PathBuf) {
        tracing::info!(
            from = %self.current_path.display(),
            to = %path.display(),
            "android pane: navigating"
        );
        self.current_path = path;
        self.selected.clear();
        self.state = AndroidPaneState::Loading;
    }

    /// Called when entries arrive from ADB.
    pub fn on_entries_loaded(
        &mut self,
        path: PathBuf,
        entries: Vec<AndroidEntry>,
        roots: Vec<PathBuf>,
    ) {
        // Discard stale responses (user may have navigated elsewhere)
        if self.current_path != path {
            tracing::debug!(
                expected = %self.current_path.display(),
                arrived = %path.display(),
                "android pane: discarding stale directory response"
            );
            return;
        }
        tracing::info!(
            path = %path.display(),
            count = entries.len(),
            roots = roots.len(),
            "android pane: directory loaded, transitioning to Browsing"
        );
        self.entries = entries;
        self.sort_entries();  // dirs first, then by active field/direction
        self.storage_roots = roots;
        self.state = AndroidPaneState::Browsing;
    }

    /// Change sort field (or toggle direction if already active) and re-sort entries.
    pub fn set_sort(&mut self, field: SortField) {
        if self.sort_by == field {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_by = field;
            self.sort_ascending = true;
        }
        self.sort_entries();
    }

    /// Sort `self.entries` in-place using the current sort state (dirs always first).
    fn sort_entries(&mut self) {
        let field = self.sort_by;
        let asc = self.sort_ascending;
        self.entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let base = match field {
                        SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                        SortField::Size => a.size.cmp(&b.size),
                        SortField::Modified => a.modified.cmp(&b.modified),
                    };
                    if asc { base } else { base.reverse() }
                }
            }
        });
    }

    /// Called when ADB returns an error for the current path.
    pub fn on_error(&mut self, msg: String) {
        tracing::warn!(
            path = %self.current_path.display(),
            error = %msg,
            "android pane: directory load error, transitioning to Error"
        );
        self.state = AndroidPaneState::Error(msg);
    }

    /// Toggle selection of an entry by index (multi-select).
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

    /// Advance the spinner animation frame.
    pub fn tick_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }

    /// Activate inline rename for the entry at `index`.
    /// No-op if the index is out of bounds.
    pub fn begin_rename(&mut self, index: usize) {
        if let Some(entry) = self.entries.get(index) {
            self.rename_pending = Some((index, entry.name.clone()));
        }
    }

    /// Update the in-flight rename input value.
    pub fn update_rename_input(&mut self, value: String) {
        if let Some((idx, _)) = &self.rename_pending {
            self.rename_pending = Some((*idx, value));
        }
    }

    /// Cancel rename without making changes.
    pub fn cancel_rename(&mut self) {
        self.rename_pending = None;
    }

    // ─── View ─────────────────────────────────────────────────────────────

    /// Render the full Android pane (header + body).
    ///
    /// `on_rename_input` is called on every keystroke while an inline rename is active.
    /// `on_rename_commit` fires when the user presses Enter to commit the rename.
    /// Full android pane view: device header + scrollable entry list with rename support.
    ///
    /// Used for the **Details** view mode.
    pub fn view<'a, Message: 'a + Clone + 'static>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_rename_input: impl Fn(String) -> Message + 'a,
        on_rename_commit: Message,
    ) -> Element<'a, Message> {
        let header = self.view_header(theme);
        let body = self.view_body(
            theme,
            on_navigate,
            on_select,
            on_rename_input,
            on_rename_commit,
        );
        column![header, body].width(Fill).height(Fill).into()
    }

    /// Compact **List** mode view: device header + sortable column header + entry rows.
    pub fn view_list<'a, Message: 'a + Clone + 'static>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_sort: impl Fn(SortField) -> Message + 'a,
    ) -> Element<'a, Message> {
        let header = self.view_header(theme);

        // When not browsing (no device, loading, error) show state-appropriate content
        if self.state != AndroidPaneState::Browsing {
            let body: Element<Message> = match &self.state {
                AndroidPaneState::NoDevice => view_connect_guide(theme),
                AndroidPaneState::Loading => container(
                    text("Loading...").size(12).color(theme.text_secondary),
                )
                .padding([12, 16])
                .into(),
                AndroidPaneState::Error(e) => container(
                    text(format!("Error: {e}")).size(12).color(theme.error),
                )
                .padding([12, 16])
                .into(),
                AndroidPaneState::Browsing => unreachable!(),
            };
            return column![header, body]
                .width(Fill)
                .height(Fill)
                .into();
        }

        let mut rows: Vec<Element<Message>> = Vec::new();

        // ".." up-navigation — stop at storage roots (e.g. /sdcard) so the user
        // can't navigate into the Android system root where listings are empty
        // or permission-denied, leaving no way to navigate back.
        let is_at_nav_root = self.current_path == std::path::Path::new("/")
            || self.storage_roots.contains(&self.current_path);
        if !is_at_nav_root {
            if let Some(parent) = self.current_path.parent() {
                let parent_path = parent.to_path_buf();
                rows.push(
                    button(
                        row![
                            text("[/]").size(11).color(theme.accent).width(28),
                            text("..").size(12).color(theme.text),
                        ]
                        .spacing(4)
                        .padding([1, 4]),
                    )
                    .width(Fill)
                    .style(|_t, _s| button::Style { background: None, ..Default::default() })
                    .on_press(on_navigate(parent_path))
                    .into(),
                );
            }
        }

        let visible_count = if self.show_hidden {
            self.entries.len()
        } else {
            self.entries.iter().filter(|e| !e.name.starts_with('.')).count()
        };

        if visible_count == 0 {
            let msg = if self.entries.is_empty() {
                "This directory is empty"
            } else {
                "All entries are hidden  (.hidden toggle to show)"
            };
            rows.push(
                container(text(msg).size(11).color(theme.text_secondary))
                    .padding([8, 12])
                    .into(),
            );
        }

        // Column pixel widths — must match between header and every data row.
        const W_ICON: u16 = 24;
        const W_TYPE: u16 = 52;
        const W_SIZE: u16 = 72;
        const W_DATE: u16 = 90;

        // ── Sortable column header ────────────────────────────────────────────
        let t = theme;
        let sort_ind = |field: SortField| -> &'static str {
            if self.sort_by == field {
                if self.sort_ascending { " ▲" } else { " ▼" }
            } else {
                ""
            }
        };
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

        let name_lbl = format!("Name{}", sort_ind(SortField::Name));
        let size_lbl = format!("Size{}", sort_ind(SortField::Size));
        let date_lbl = format!("Modified{}", sort_ind(SortField::Modified));

        let col_header = container(
            row![
                iced::widget::Space::new(20, 1),
                text("").width(W_ICON),
                mk_hdr(name_lbl, SortField::Name, Fill, on_sort(SortField::Name)),
                mk_hdr("Type".to_string(), SortField::Name, iced::Length::Fixed(W_TYPE as f32), on_sort(SortField::Name)),
                mk_hdr(size_lbl, SortField::Size, iced::Length::Fixed(W_SIZE as f32), on_sort(SortField::Size)),
                mk_hdr(date_lbl, SortField::Modified, iced::Length::Fixed(W_DATE as f32), on_sort(SortField::Modified)),
            ]
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

        rows.insert(0, col_header.into());

        for (i, entry) in self.entries.iter().enumerate() {
            if !self.show_hidden && entry.name.starts_with('.') {
                continue;
            }
            let is_selected = self.selected.contains(&i);
            let bg: Option<iced::Background> =
                if is_selected { Some(theme.accent.scale_alpha(0.2).into()) } else { None };
            let is_hidden = entry.name.starts_with('.');
            let fg = if is_hidden { theme.text_secondary } else { theme.text };
            let icon_fg = if entry.is_dir { theme.accent } else { theme.text_secondary };
            let icon = if entry.is_symlink { "[@]" } else if entry.is_dir { "[/]" } else { "[-]" };
            let type_label = if entry.is_symlink { "Symlink" } else if entry.is_dir { "Folder" } else {
                entry.name.rsplit('.').next()
                    .filter(|e| !e.is_empty() && *e != entry.name.as_str())
                    .unwrap_or("File")
            };
            let size_str = entry.size_display();
            let date_str = entry.modified.clone();
            let name = entry.name.clone();
            let entry_path = self.current_path.join(&name);
            // Treat symlinks as navigable — on Android, /sdcard and storage
            // mount points are symlinks that point to directories.
            let can_navigate = entry.is_dir || entry.is_symlink;
            let on_nav = on_navigate(entry_path);
            let on_sel_btn = on_select(i);
            let on_sel_chk = on_select(i);
            rows.push(
                container(
                    row![
                        checkbox("", is_selected)
                            .on_toggle(move |_| on_sel_chk.clone())
                            .size(12),
                        button(
                            row![
                                text(icon).size(11).color(icon_fg).width(W_ICON),
                                text(name).size(11).color(fg).width(Fill),
                                text(type_label).size(10).color(theme.text_secondary).width(W_TYPE),
                                text(size_str).size(10).color(theme.text_secondary).width(W_SIZE),
                                text(date_str).size(10).color(theme.text_secondary).width(W_DATE),
                            ]
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
                .into(),
            );
        }

        let body = container(
            scrollable(column(rows).width(Fill))
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        });

        column![header, body].width(Fill).height(Fill).into()
    }

    fn view_header<'a, Message: 'a + Clone>(&'a self, theme: ThemeColors) -> Element<'a, Message> {
        let header_text = match &self.state {
            AndroidPaneState::NoDevice => "Android Device — No device connected".to_string(),
            AndroidPaneState::Loading => {
                let dots = match self.spinner_frame % 4 {
                    0 => ".",
                    1 => "..",
                    2 => "...",
                    _ => "",
                };
                format!("Android Device - Loading{}", dots)
            }
            AndroidPaneState::Browsing => {
                let path = self.current_path.to_string_lossy();
                let display = if path.len() > 35 {
                    format!("...{}", &path[path.len() - 35..])
                } else {
                    path.into_owned()
                };
                format!("Android Device  {}", display)
            }
            AndroidPaneState::Error(msg) => format!("Android Device - Error: {}", msg),
        };

        container(text(header_text).size(12).color(theme.text_secondary))
            .width(Fill)
            .height(28.0)
            .padding([6, 12])
            .style(move |_t| container::Style {
                background: Some(theme.background_secondary.into()),
                border: Border {
                    color: theme.border,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn view_body<'a, Message: 'a + Clone + 'static>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_rename_input: impl Fn(String) -> Message + 'a,
        on_rename_commit: Message,
    ) -> Element<'a, Message> {
        match &self.state {
            AndroidPaneState::NoDevice => view_connect_guide(theme),

            AndroidPaneState::Loading => self.view_loading(theme),

            AndroidPaneState::Error(msg) => self.view_empty(theme, "Error", msg),

            AndroidPaneState::Browsing => self.view_entries(
                theme,
                on_navigate,
                on_select,
                on_rename_input,
                on_rename_commit,
            ),
        }
    }

    fn view_empty<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        title: &'a str,
        subtitle: &'a str,
    ) -> Element<'a, Message> {
        let content = column![
            text(title).size(13).color(theme.text_secondary),
            text(subtitle).size(11).color(theme.text_secondary),
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


    fn view_loading<'a, Message: 'a + Clone>(&'a self, theme: ThemeColors) -> Element<'a, Message> {
        let spinner_chars = ["|", "/", "-", "\\"];
        let spinner = spinner_chars[(self.spinner_frame as usize) % spinner_chars.len()];

        let content = row![
            text(spinner).size(14).color(theme.accent),
            text("  Fetching directory listing...")
                .size(12)
                .color(theme.text_secondary),
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

    fn view_entries<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_rename_input: impl Fn(String) -> Message + 'a,
        on_rename_commit: Message,
    ) -> Element<'a, Message> {
        // Column header
        let col_header = container(
            row![
                text("Name")
                    .size(11)
                    .color(theme.text_secondary)
                    .width(Fill),
                text("Size").size(11).color(theme.text_secondary).width(80),
                text("Modified")
                    .size(11)
                    .color(theme.text_secondary)
                    .width(90),
            ]
            .padding([2, 12])
            .spacing(4),
        )
        .width(Fill)
        .style(move |_t| container::Style {
            background: Some(theme.background_secondary.into()),
            border: Border {
                color: theme.border,
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        });

        // Pre-build the inline rename input (consumes callbacks once).
        // Only one entry can be in rename mode at a time, so this element
        // is moved into the row at the right index via Option::take().
        let mut rename_row: Option<(usize, Element<Message>)> =
            if let Some((idx, val)) = &self.rename_pending {
                let icon = self
                    .entries
                    .get(*idx)
                    .map(|e| {
                        if e.is_symlink {
                            "[@]"
                        } else if e.is_dir {
                            "[/]"
                        } else {
                            "[-]"
                        }
                    })
                    .unwrap_or("[-]");
                let input = text_input("New name…", val.as_str())
                    .id(text_input::Id::new(RENAME_INPUT_ID))
                    .on_input(on_rename_input)
                    .on_submit(on_rename_commit)
                    .size(12)
                    .width(Fill);
                let elem: Element<Message> = row![text(icon).size(12), input]
                    .spacing(6)
                    .padding([1, 0])
                    .into();
                Some((*idx, elem))
            } else {
                drop((on_rename_input, on_rename_commit));
                None
            };

        let mut rows: Vec<Element<Message>> = Vec::new();

        // ".." up-navigation — allow all the way to filesystem root "/"
        let is_at_root = self.current_path == std::path::Path::new("/");

        if !is_at_root {
            if let Some(parent) = self.current_path.parent() {
                let parent_path = parent.to_path_buf();
                let up_btn = button(
                    row![
                        text("[/]").size(11).color(theme.accent).width(28),
                        text("..").size(12).color(theme.text),
                        iced::widget::Space::with_width(Fill),
                        text("").size(12).width(80),
                        text("").size(12).width(90),
                    ]
                    .spacing(6)
                    .padding([1, 0]),
                )
                .width(Fill)
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(on_navigate(parent_path));
                rows.push(up_btn.into());
            }
        }

        // Storage root quick-nav (shown at /sdcard level)
        if self.current_path == std::path::Path::new("/sdcard") && self.storage_roots.len() > 1 {
            for root in &self.storage_roots {
                if root == &PathBuf::from("/sdcard") {
                    continue; // already here
                }
                let root_name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.to_string_lossy().into_owned());
                let root_path = root.clone();
                let btn = button(
                    row![
                        text("💾").size(12),
                        text(format!("{} (SD)", root_name))
                            .size(12)
                            .color(theme.accent),
                        iced::widget::Space::with_width(Fill),
                        text("--").size(11).color(theme.text_secondary).width(80),
                        text("").size(11).color(theme.text_secondary).width(90),
                    ]
                    .spacing(6)
                    .padding([1, 0]),
                )
                .width(Fill)
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(on_navigate(root_path));
                rows.push(btn.into());
            }
        }

        // Empty directory message (also shown when all entries are filtered as hidden)
        let visible_count = if self.show_hidden {
            self.entries.len()
        } else {
            self.entries.iter().filter(|e| !e.name.starts_with('.')).count()
        };
        if visible_count == 0 {
            let msg = if self.entries.is_empty() {
                "This directory is empty"
            } else {
                "All entries are hidden  (.hidden toggle to show)"
            };
            rows.push(
                container(text(msg).size(11).color(theme.text_secondary))
                    .padding([8, 12])
                    .into(),
            );
        }

        // File/directory entries (skip hidden entries when show_hidden is false)
        for (i, entry) in self.entries.iter().enumerate() {
            if !self.show_hidden && entry.name.starts_with('.') {
                continue;
            }
            let is_selected = self.selected.contains(&i);
            let row_bg: Option<Color> = if is_selected {
                Some(theme.accent.scale_alpha(0.2))
            } else {
                None
            };

            let icon = if entry.is_symlink {
                "[@]"
            } else if entry.is_dir {
                "[/]"
            } else {
                "[-]"
            };
            let icon_fg = if entry.is_dir || entry.is_symlink {
                theme.accent
            } else {
                theme.text_secondary
            };

            let name_color = if entry.is_hidden {
                theme.text_secondary
            } else {
                theme.text
            };

            let entry_path = entry.path.clone();
            let entry_is_dir = entry.is_dir || entry.is_symlink;
            let on_nav = on_navigate(entry_path.clone());
            let on_sel_btn = on_select(i);
            let on_sel_chk = on_select(i);

            // If this entry is the one being renamed, use the pre-built input row.
            let row_element: Element<Message> = if rename_row
                .as_ref()
                .map(|(idx, _)| *idx == i)
                .unwrap_or(false)
            {
                rename_row.take().unwrap().1
            } else {
                let row_content = row![
                    text(icon).size(11).color(icon_fg).width(28),
                    text(entry.name.clone())
                        .size(12)
                        .color(name_color)
                        .width(Fill),
                    text(entry.size_display())
                        .size(11)
                        .color(theme.text_secondary)
                        .width(80),
                    text(entry.modified.clone())
                        .size(11)
                        .color(theme.text_secondary)
                        .width(90),
                ]
                .spacing(6)
                .padding([1, 0]);

                container(
                    row![
                        checkbox("", is_selected)
                            .on_toggle(move |_| on_sel_chk.clone())
                            .size(14),
                        button(row_content)
                            .width(Fill)
                            .style(|_t, _s| button::Style {
                                background: None,
                                ..Default::default()
                            })
                            .on_press(if entry_is_dir { on_nav } else { on_sel_btn }),
                    ]
                    .spacing(6)
                    .padding([1, 4])
                    .align_y(iced::Alignment::Center),
                )
                .width(Fill)
                .style(move |_t| container::Style {
                    background: row_bg.map(Into::into),
                    ..Default::default()
                })
                .into()
            };

            rows.push(row_element);
        }

        let list = scrollable(column(rows).width(Fill).padding([0, 4]))
            .width(Fill)
            .height(Fill);

        container(column![col_header, list].width(Fill).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(move |_t| container::Style {
                background: Some(theme.background.into()),
                ..Default::default()
            })
            .into()
    }
}

// ─── Device connection guide ───────────────────────────────────────────────────

/// Renders the step-by-step guide shown in the Android pane when no device is connected.
fn view_connect_guide<Message: Clone + 'static>(theme: ThemeColors) -> Element<'static, Message>
where
    Message: 'static,
{
    use iced::widget::Space;

    fn step<Message: Clone + 'static>(
        theme: ThemeColors,
        number: &'static str,
        title: &'static str,
        detail: &'static str,
    ) -> Element<'static, Message> {
        container(
            column![
                row![
                    text(number).size(11).color(theme.accent),
                    text(title).size(12).color(theme.text),
                ]
                .spacing(6),
                text(detail).size(11).color(theme.text_secondary),
            ]
            .spacing(2),
        )
        .padding([6, 10])
        .style(move |_t| container::Style {
            background: Some(theme.background_secondary.into()),
            border: iced::Border {
                color: theme.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .width(Fill)
        .into()
    }

    let guide = column![
        text("Connect your Android device").size(14).color(theme.text),
        Space::with_height(4),
        text("ADB requires Developer Mode to be enabled on your phone.")
            .size(11)
            .color(theme.text_secondary),
        Space::with_height(12),

        // ── Step 1 ──────────────────────────────────────────────────────────
        text("Step 1 — Unlock Developer Options")
            .size(11)
            .color(theme.text_secondary),
        Space::with_height(4),
        step::<Message>(
            theme,
            "1.",
            "Open Settings › About Phone › Software Information",
            "The exact path varies by manufacturer — see notes below.",
        ),
        step::<Message>(
            theme,
            "2.",
            "Tap \"Build Number\" seven times in a row",
            "A toast will say \"You are now a developer!\" when done.",
        ),
        Space::with_height(10),

        // ── Step 2 ──────────────────────────────────────────────────────────
        text("Step 2 — Enable USB Debugging")
            .size(11)
            .color(theme.text_secondary),
        Space::with_height(4),
        step::<Message>(
            theme,
            "3.",
            "Go back to Settings › Developer Options",
            "This menu appears after step 1.",
        ),
        step::<Message>(
            theme,
            "4.",
            "Turn on \"USB Debugging\"",
            "Toggle is near the top of the Developer Options list.",
        ),
        Space::with_height(10),

        // ── Step 3 ──────────────────────────────────────────────────────────
        text("Step 3 — Connect and authorize")
            .size(11)
            .color(theme.text_secondary),
        Space::with_height(4),
        step::<Message>(
            theme,
            "5.",
            "Plug in a USB cable between your phone and this computer",
            "Use a data cable — charge-only cables won't work.",
        ),
        step::<Message>(
            theme,
            "6.",
            "Set USB mode to \"File Transfer\" on your phone",
            "Swipe down the notification shade and tap the USB notification.",
        ),
        step::<Message>(
            theme,
            "7.",
            "Tap \"Allow\" on the \"Allow USB Debugging?\" dialog",
            "Tick \"Always allow from this computer\" to skip this next time.",
        ),
        Space::with_height(12),

        // ── Manufacturer notes ───────────────────────────────────────────────
        text("Manufacturer notes — where to find Build Number")
            .size(11)
            .color(theme.text_secondary),
        Space::with_height(4),
        container(
            column![
                text("Samsung:   Settings › About Phone › Software Information › Build Number")
                    .size(10).color(theme.text_secondary),
                text("Pixel:     Settings › About Phone › Build Number")
                    .size(10).color(theme.text_secondary),
                text("OnePlus:   Settings › About Device › Version › Build Number")
                    .size(10).color(theme.text_secondary),
                text("Xiaomi:    Settings › About Phone › MIUI Version (tap 7×)")
                    .size(10).color(theme.text_secondary),
                text("Motorola:  Settings › About Phone › Build Number")
                    .size(10).color(theme.text_secondary),
            ]
            .spacing(3),
        )
        .padding([8, 10])
        .style(move |_t| container::Style {
            background: Some(theme.background_secondary.into()),
            border: iced::Border {
                color: theme.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .width(Fill),
    ]
    .spacing(4)
    .width(Fill);

    container(scrollable(guide.padding([0, 4])))
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        })
        .into()
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::AndroidEntry;
    use std::path::PathBuf;

    fn make_entry(name: &str, is_dir: bool) -> AndroidEntry {
        AndroidEntry {
            name: name.to_string(),
            path: PathBuf::from("/sdcard").join(name),
            size: if is_dir { 0 } else { 1024 },
            modified: "2024-01-15".to_string(),
            is_dir,
            is_symlink: false,
            is_hidden: name.starts_with('.'),
        }
    }

    #[test]
    fn default_state_is_no_device() {
        let pane = AndroidPane::default();
        assert_eq!(pane.state, AndroidPaneState::NoDevice);
        assert!(pane.entries.is_empty());
    }

    #[test]
    fn on_device_connected_transitions_to_loading() {
        let mut pane = AndroidPane::default();
        pane.on_device_connected();
        assert_eq!(pane.state, AndroidPaneState::Loading);
        assert_eq!(pane.current_path, PathBuf::from("/sdcard"));
    }

    #[test]
    fn on_device_disconnected_returns_to_no_device() {
        let mut pane = AndroidPane::default();
        pane.on_device_connected();
        pane.on_device_disconnected();
        assert_eq!(pane.state, AndroidPaneState::NoDevice);
        assert!(pane.entries.is_empty());
        assert!(pane.storage_roots.is_empty());
    }

    #[test]
    fn begin_navigate_transitions_to_loading() {
        let mut pane = AndroidPane::default();
        pane.begin_navigate(PathBuf::from("/sdcard/DCIM"));
        assert_eq!(pane.state, AndroidPaneState::Loading);
        assert_eq!(pane.current_path, PathBuf::from("/sdcard/DCIM"));
        assert!(pane.selected.is_empty());
    }

    #[test]
    fn on_entries_loaded_transitions_to_browsing() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());

        let entries = vec![make_entry("DCIM", true), make_entry("photo.jpg", false)];
        let roots = vec![PathBuf::from("/sdcard")];
        pane.on_entries_loaded(path, entries, roots);

        assert_eq!(pane.state, AndroidPaneState::Browsing);
        assert_eq!(pane.entries.len(), 2);
        // dirs-first sort: DCIM before photo.jpg
        assert!(pane.entries[0].is_dir);
    }

    #[test]
    fn on_entries_loaded_stale_response_ignored() {
        let mut pane = AndroidPane::default();
        pane.begin_navigate(PathBuf::from("/sdcard/DCIM")); // navigated here

        // Response arrives for /sdcard (old path)
        let stale_entries = vec![make_entry("old.jpg", false)];
        pane.on_entries_loaded(PathBuf::from("/sdcard"), stale_entries, vec![]);

        // State should still be Loading (not Browsing), entries unchanged
        assert_eq!(pane.state, AndroidPaneState::Loading);
        assert!(pane.entries.is_empty());
    }

    #[test]
    fn on_error_transitions_to_error() {
        let mut pane = AndroidPane::default();
        pane.begin_navigate(PathBuf::from("/sdcard/private"));
        pane.on_error("Permission denied".to_string());
        assert!(matches!(pane.state, AndroidPaneState::Error(_)));
    }

    #[test]
    fn select_toggles_entry() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());
        pane.on_entries_loaded(
            path,
            vec![make_entry("DCIM", true), make_entry("Music", true)],
            vec![],
        );

        // First click — selects
        pane.select(1);
        assert!(pane.selected.contains(&1));

        // Second click — deselects
        pane.select(1);
        assert!(!pane.selected.contains(&1));
    }

    #[test]
    fn select_multi_adds_both() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());
        pane.on_entries_loaded(
            path,
            vec![make_entry("DCIM", true), make_entry("Music", true)],
            vec![],
        );

        pane.select(0);
        pane.select(1);
        assert!(pane.selected.contains(&0));
        assert!(pane.selected.contains(&1));
    }

    #[test]
    fn select_out_of_bounds_ignored() {
        let mut pane = AndroidPane::default();
        pane.select(99);
        assert!(pane.selected.is_empty());
    }

    #[test]
    fn tick_spinner_advances_and_wraps() {
        let mut pane = AndroidPane::default();
        assert_eq!(pane.spinner_frame, 0);
        for _ in 0..255 {
            pane.tick_spinner();
        }
        // After 255 ticks, wrapping_add(1) gives 0 on the next tick
        pane.tick_spinner();
        assert_eq!(pane.spinner_frame, 0);
    }

    #[test]
    fn entries_sorted_dirs_first() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());
        let entries = vec![
            make_entry("zebra.txt", false),
            make_entry("alpha_dir", true),
            make_entry("beta_dir", true),
        ];
        pane.on_entries_loaded(path, entries, vec![]);
        assert!(pane.entries[0].is_dir);
        assert!(pane.entries[1].is_dir);
        assert!(!pane.entries[2].is_dir);
        assert_eq!(pane.entries[0].name, "alpha_dir");
    }
}
