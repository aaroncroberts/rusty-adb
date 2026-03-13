//! Local filesystem pane
//!
//! Implements the left pane of the dual-pane view: a file browser for the
//! macOS (host) filesystem using `std::fs`.
//!
//! # Architecture
//! `LocalPane` owns all state (current path, entries, selection, sort, hidden).
//! It exposes a `view()` method that renders an `Element<Message>` and a set
//! of message handlers called by `App::update`.

use std::path::PathBuf;
use std::time::SystemTime;

use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{Border, Color, Element, Fill, Length};

use crate::theme::ThemeColors;

// ─── Data Model ───────────────────────────────────────────────────────────────

/// A single entry (file or directory) in the local filesystem listing
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_dir: bool,
    pub is_hidden: bool,
}

impl FileEntry {
    /// Human-readable file size: directories show "--", files show KB/MB/GB.
    pub fn size_display(&self) -> String {
        if self.is_dir {
            return "--".to_string();
        }
        format_size(self.size)
    }

    /// Human-readable last-modified date (local time, date only).
    pub fn modified_display(&self) -> String {
        self.modified
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| format_unix_date(d.as_secs()))
            .unwrap_or_else(|| "--".to_string())
    }
}

/// Field to sort entries by (Name/Size/Modified — all now active via sort header clicks)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    Size,
    Modified,
}

// ─── Pane State ───────────────────────────────────────────────────────────────

/// State for the local filesystem pane
#[derive(Debug, Clone)]
pub struct LocalPane {
    /// Currently displayed directory
    pub current_path: PathBuf,
    /// All entries in `current_path` (after filtering)
    pub entries: Vec<FileEntry>,
    /// Selected entry indices (multi-select; click toggles in/out)
    pub selected: Vec<usize>,
    /// Whether to show hidden files (names starting with '.')
    pub show_hidden: bool,
    /// Active sort field (always dirs-first within sort)
    pub sort_by: SortField,
    /// True = ascending (A→Z, small→large); false = descending
    pub sort_ascending: bool,
    /// Last error loading directory (displayed in body)
    pub error: Option<String>,
    /// Show the Type column in list view
    pub show_type: bool,
    /// Show the Size column in list view
    pub show_size: bool,
    /// Show the Modified column in list view
    pub show_modified: bool,
}

impl LocalPane {
    /// Create a new pane rooted at `path` and load its contents.
    pub fn new(path: PathBuf) -> Self {
        let mut pane = Self {
            current_path: path,
            entries: Vec::new(),
            selected: Vec::new(),
            show_hidden: false,
            sort_by: SortField::Name,
            sort_ascending: true,
            error: None,
            show_type: true,
            show_size: true,
            show_modified: true,
        };
        pane.reload();
        pane
    }

    /// Re-read the current directory from disk and update `entries`.
    pub fn reload(&mut self) {
        self.selected.clear();
        self.error = None;

        match load_entries(
            &self.current_path,
            self.show_hidden,
            self.sort_by,
            self.sort_ascending,
        ) {
            Ok(entries) => {
                tracing::debug!(
                    path = %self.current_path.display(),
                    count = entries.len(),
                    show_hidden = self.show_hidden,
                    "local directory loaded"
                );
                self.entries = entries;
            }
            Err(e) => {
                tracing::warn!(
                    path = %self.current_path.display(),
                    error = %e,
                    "failed to read local directory"
                );
                self.entries = Vec::new();
                self.error = Some(e);
            }
        }
    }

    /// Navigate into a subdirectory or handle ".." navigation.
    pub fn navigate_to(&mut self, path: PathBuf) {
        tracing::info!(
            from = %self.current_path.display(),
            to = %path.display(),
            "local pane navigating"
        );
        self.current_path = path;
        self.reload();
    }

    /// Toggle selection of an entry by index (multi-select).
    ///
    /// - If index is already in `selected`, it is removed (deselect).
    /// - Otherwise it is added.
    /// - Out-of-bounds indices are ignored.
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

    /// Toggle hidden file visibility and reload.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        tracing::debug!(show_hidden = self.show_hidden, "toggled hidden files");
        self.reload();
    }

    /// Toggle Type column visibility.
    pub fn toggle_type(&mut self) { self.show_type = !self.show_type; }
    /// Toggle Size column visibility.
    pub fn toggle_size(&mut self) { self.show_size = !self.show_size; }
    /// Toggle Modified column visibility.
    pub fn toggle_modified(&mut self) { self.show_modified = !self.show_modified; }

    /// Change sort field (or toggle direction if already active) and reload.
    ///
    /// - Same field clicked twice → toggle ascending/descending
    /// - New field clicked → set field, reset to ascending
    pub fn sort_by(&mut self, field: SortField) {
        if self.sort_by == field {
            self.sort_ascending = !self.sort_ascending;
            tracing::debug!(sort = ?field, ascending = self.sort_ascending, "sort direction toggled");
        } else {
            self.sort_by = field;
            self.sort_ascending = true;
            tracing::debug!(sort = ?field, "sort field changed");
        }
        self.reload();
    }

    // ─── View ─────────────────────────────────────────────────────────────

    /// Render the local pane with full columns (header + breadcrumb + Name/Size/Modified table).
    ///
    /// Used for the **Details** view mode — sortable columns, size and date visible.
    pub fn view<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_toggle_hidden: Message,
        on_sort: impl Fn(SortField) -> Message + 'a,
    ) -> Element<'a, Message> {
        let header = self.view_header(theme, on_toggle_hidden);
        let breadcrumb = view_breadcrumb(&self.current_path, theme, &on_navigate);
        let body = self.view_body(theme, on_navigate, on_select, on_sort);
        column![header, breadcrumb, body]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Render the local pane in compact **List** mode: breadcrumb + name-only rows.
    ///
    /// No size or date columns — maximum entry density.
    pub fn view_list<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_sort: impl Fn(SortField) -> Message + 'a,
    ) -> Element<'a, Message> {
        // Column pixel widths — must match between header and every data row.
        const W_ICON: u16 = 24;
        const W_TYPE: u16 = 52;
        const W_SIZE: u16 = 72;
        const W_DATE: u16 = 90;

        let t = theme;
        let breadcrumb = view_breadcrumb(&self.current_path, theme, &on_navigate);

        // ── Sort indicator helper ─────────────────────────────────────────────
        let sort_ind = |field: SortField| -> &'static str {
            if self.sort_by == field {
                if self.sort_ascending { " ▲" } else { " ▼" }
            } else {
                ""
            }
        };
        let mk_col_hdr = |label: String, field: SortField, width: iced::Length, msg: Message|
            -> Element<'a, Message>
        {
            let is_active = self.sort_by == field;
            let fg = if is_active { t.accent } else { t.text_secondary };
            button(
                text(label).size(10).color(fg),
            )
            .width(width)
            .padding([2, 4])
            .style(move |_t, _s| button::Style { background: None, ..Default::default() })
            .on_press(msg)
            .into()
        };

        // ── Sticky column header ──────────────────────────────────────────────
        let name_label = format!("Name{}", sort_ind(SortField::Name));
        let size_label = format!("Size{}", sort_ind(SortField::Size));
        let date_label = format!("Modified{}", sort_ind(SortField::Modified));

        let mut hdr_cells: Vec<Element<Message>> = vec![
            iced::widget::Space::new(20, 1).into(),
            text("").width(W_ICON).into(),
            mk_col_hdr(name_label, SortField::Name, Fill, on_sort(SortField::Name)),
        ];
        if self.show_type {
            hdr_cells.push(mk_col_hdr("Type".to_string(), SortField::Name, iced::Length::Fixed(W_TYPE as f32), on_sort(SortField::Name)));
        }
        if self.show_size {
            hdr_cells.push(mk_col_hdr(size_label, SortField::Size, iced::Length::Fixed(W_SIZE as f32), on_sort(SortField::Size)));
        }
        if self.show_modified {
            hdr_cells.push(mk_col_hdr(date_label, SortField::Modified, iced::Length::Fixed(W_DATE as f32), on_sort(SortField::Modified)));
        }

        let header = container(
            iced::widget::Row::from_vec(hdr_cells)
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

        // ── Rows ──────────────────────────────────────────────────────────────
        let mut rows: Vec<Element<Message>> = Vec::new();

        // ".." up-navigation
        if let Some(parent) = self.current_path.parent() {
            let parent = parent.to_path_buf();
            rows.push(
                button(
                    row![
                        text("..").size(12).color(t.text_secondary).width(Fill),
                    ]
                    .padding([2, 8]),
                )
                .width(Fill)
                .style(|_t, _s| button::Style { background: None, ..Default::default() })
                .on_press(on_navigate(parent))
                .into(),
            );
        }

        if self.entries.is_empty() {
            rows.push(
                container(text("This folder is empty").size(11).color(t.text_secondary))
                    .padding([8, 12])
                    .into(),
            );
        }

        for (i, entry) in self.entries.iter().enumerate() {
            let is_selected = self.selected.contains(&i);
            let bg: Option<iced::Background> =
                if is_selected { Some(t.accent.scale_alpha(0.2).into()) } else { None };
            let fg = if entry.is_hidden { t.text_secondary } else { t.text };
            let icon_fg = if entry.is_dir { t.accent } else { t.text_secondary };
            let icon = if entry.is_dir { "[/]" } else { "[-]" };
            let type_label = if entry.is_dir { "Folder" } else {
                entry.name.rsplit('.').next()
                    .filter(|e| !e.is_empty() && *e != &entry.name)
                    .map(|e| e)
                    .unwrap_or("File")
            };
            let size_str = entry.size_display();
            let date_str = entry.modified_display();
            let name = entry.name.clone();
            let entry_path = entry.path.clone();
            let entry_is_dir = entry.is_dir;
            let on_nav = on_navigate(entry_path);
            let on_sel_btn = on_select(i);
            let on_sel_chk = on_select(i);

            let mut cells: Vec<Element<Message>> = vec![
                text(icon).size(11).color(icon_fg).width(W_ICON).into(),
                text(name).size(11).color(fg).width(Fill).into(),
            ];
            if self.show_type {
                cells.push(text(type_label).size(10).color(t.text_secondary).width(W_TYPE).into());
            }
            if self.show_size {
                cells.push(text(size_str).size(10).color(t.text_secondary).width(W_SIZE).into());
            }
            if self.show_modified {
                cells.push(text(date_str).size(10).color(t.text_secondary).width(W_DATE).into());
            }

            rows.push(
                container(
                    row![
                        checkbox("", is_selected)
                            .on_toggle(move |_| on_sel_chk.clone())
                            .size(12),
                        button(
                            iced::widget::Row::from_vec(cells)
                            .spacing(6)
                            .align_y(iced::Alignment::Center),
                        )
                        .width(Fill)
                        .style(|_t, _s| button::Style { background: None, ..Default::default() })
                        .on_press(if entry_is_dir { on_nav } else { on_sel_btn }),
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
            background: Some(t.background.into()),
            ..Default::default()
        });

        column![breadcrumb, header, body].width(Fill).height(Fill).into()
    }

    fn view_header<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_toggle_hidden: Message,
    ) -> Element<'a, Message> {
        let hidden_label = if self.show_hidden { "Hide ." } else { "Show ." };

        let hidden_btn = button(text(hidden_label).size(11).color(theme.text_secondary))
            .style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .on_press(on_toggle_hidden);

        let content = row![
            text("Local Files").size(12).color(theme.text_secondary),
            iced::widget::Space::with_width(Fill),
            hidden_btn,
        ]
        .align_y(iced::Alignment::Center)
        .padding([0, 8]);

        container(content)
            .width(Fill)
            .height(28.0)
            .padding([4, 8])
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

    fn view_body<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_sort: impl Fn(SortField) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(err) = &self.error {
            let msg = text(format!("Error: {}", err)).size(12).color(theme.error);
            return container(msg)
                .width(Fill)
                .height(Fill)
                .padding(16)
                .style(move |_t| container::Style {
                    background: Some(theme.background.into()),
                    ..Default::default()
                })
                .into();
        }

        // ── Sort indicator helper ────────────────────────────────────────────
        let sort_indicator = |field: SortField| -> &'static str {
            if self.sort_by == field {
                if self.sort_ascending {
                    " ▲"
                } else {
                    " ▼"
                }
            } else {
                ""
            }
        };

        // ── Clickable column headers ─────────────────────────────────────────
        let name_hdr_label = format!("Name{}", sort_indicator(SortField::Name));
        let size_hdr_label = format!("Size{}", sort_indicator(SortField::Size));
        let date_hdr_label = format!("Modified{}", sort_indicator(SortField::Modified));

        let mk_hdr_btn = |label: String,
                          _field: SortField,
                          width: Length,
                          msg: Message|
         -> Element<'a, Message> {
            button(
                text(label)
                    .size(11)
                    .color(theme.text_secondary)
                    .width(width),
            )
            .style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .padding([2, 4])
            .on_press(msg)
            .into()
        };

        let col_header = row![
            mk_hdr_btn(
                name_hdr_label,
                SortField::Name,
                Length::Fill,
                on_sort(SortField::Name)
            ),
            mk_hdr_btn(
                size_hdr_label,
                SortField::Size,
                Length::Fixed(80.0),
                on_sort(SortField::Size)
            ),
            mk_hdr_btn(
                date_hdr_label,
                SortField::Modified,
                Length::Fixed(90.0),
                on_sort(SortField::Modified)
            ),
        ]
        .padding([2, 8])
        .spacing(4);

        let col_header_container =
            container(col_header)
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

        // ".." up-navigation entry (if not at filesystem root)
        let mut rows: Vec<Element<Message>> = Vec::new();

        if self.current_path.parent().is_some() {
            let parent = self.current_path.parent().unwrap().to_path_buf();
            let up_btn = button(
                row![
                    text("📁").size(12),
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
            .on_press(on_navigate(parent));
            rows.push(up_btn.into());
        }

        // Empty directory message
        if self.entries.is_empty() {
            rows.push(
                container(
                    text("This folder is empty")
                        .size(11)
                        .color(theme.text_secondary),
                )
                .padding([8, 12])
                .into(),
            );
        }

        // File/directory entries
        for (i, entry) in self.entries.iter().enumerate() {
            let is_selected = self.selected.contains(&i);
            let row_bg: Option<Color> = if is_selected {
                Some(theme.accent.scale_alpha(0.2))
            } else {
                None
            };

            let icon = if entry.is_dir { "[/]" } else { "[-]" };
            let icon_fg = if entry.is_dir { theme.accent } else { theme.text_secondary };
            let name_color = if entry.is_hidden {
                theme.text_secondary
            } else {
                theme.text
            };

            let entry_path = entry.path.clone();
            let entry_is_dir = entry.is_dir;

            let on_nav = on_navigate(entry_path.clone());
            let on_sel_btn = on_select(i);
            let on_sel_chk = on_select(i);

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
                text(entry.modified_display())
                    .size(11)
                    .color(theme.text_secondary)
                    .width(90),
            ]
            .spacing(6)
            .padding([1, 0]);

            let entry_row = container(
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
            });

            rows.push(entry_row.into());
        }

        let list = scrollable(column(rows).width(Fill).padding([0, 4]))
            .width(Fill)
            .height(Fill);

        let body = container(column![col_header_container, list].width(Fill).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(move |_t| container::Style {
                background: Some(theme.background.into()),
                ..Default::default()
            });

        body.into()
    }
}

// ─── Breadcrumb ───────────────────────────────────────────────────────────────

/// Render a clickable breadcrumb bar for `current_path`.
///
/// e.g.  `/` › `Users` › `aaron` › `Documents`
///
/// Each segment (except the last) emits `on_navigate(ancestor_path)` when clicked.
/// The buttons store computed `Message` values (not closure references), so this
/// function's lifetime is independent of the caller's `&'a self`.
fn view_breadcrumb<'a, Message: 'a + Clone>(
    current_path: &std::path::Path,
    theme: ThemeColors,
    on_navigate: &impl Fn(PathBuf) -> Message,
) -> Element<'a, Message> {
    // Collect ancestors in root-first order
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
    let mut crumb_row: Vec<Element<Message>> = Vec::new();

    for (i, (label, path)) in segments.into_iter().enumerate() {
        let is_last = i == last_idx;
        if is_last {
            crumb_row.push(text(label).size(11).color(theme.accent).into());
        } else {
            // Compute the Message value eagerly — the button owns it, not a closure ref.
            let nav_msg = on_navigate(path);
            let btn = button(text(label).size(11).color(theme.text_secondary))
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
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
            border: Border {
                color: theme.border,
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

// ─── Filesystem Loading ────────────────────────────────────────────────────────

/// Read directory entries, filter hidden files, and sort.
fn load_entries(
    path: &PathBuf,
    show_hidden: bool,
    sort: SortField,
    ascending: bool,
) -> Result<Vec<FileEntry>, String> {
    let read = std::fs::read_dir(path).map_err(|e| e.to_string())?;

    let mut entries: Vec<FileEntry> = read
        .filter_map(|res| res.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_hidden = name.starts_with('.');
            if is_hidden && !show_hidden {
                return None;
            }
            let meta = entry.metadata().ok()?;
            Some(FileEntry {
                path: entry.path(),
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified: meta.modified().ok(),
                is_hidden,
                name,
            })
        })
        .collect();

    // Dirs first, then sort within groups (with direction)
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => {
            let base = match sort {
                SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortField::Size => a.size.cmp(&b.size),
                SortField::Modified => a.modified.cmp(&b.modified),
            };
            if ascending {
                base
            } else {
                base.reverse()
            }
        }
    });

    Ok(entries)
}

// ─── Formatting Helpers ────────────────────────────────────────────────────────

/// Format a byte count into human-readable form.
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Convert a Unix timestamp (seconds) to a YYYY-MM-DD string (UTC).
fn format_unix_date(unix_secs: u64) -> String {
    // Simple hand-rolled conversion (avoids pulling in chrono for a date stamp)
    let days_since_epoch = unix_secs / 86400;
    // Compute year/month/day from days since 1970-01-01 (proleptic Gregorian)
    let (year, month, day) = days_to_ymd(days_since_epoch as i64);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Gregorian calendar: convert days since 1970-01-01 to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn format_size_kb() {
        assert_eq!(format_size(1024), "1 KB");
        assert_eq!(format_size(1536), "2 KB");
    }

    #[test]
    fn format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn format_date_known() {
        // 2024-01-15 = days since epoch 19737
        // Unix timestamp for 2024-01-15 00:00:00 UTC = 1705276800
        assert_eq!(format_unix_date(1705276800), "2024-01-15");
    }

    #[test]
    fn format_date_epoch() {
        assert_eq!(format_unix_date(0), "1970-01-01");
    }

    #[test]
    fn load_entries_valid_path() {
        let path = std::env::temp_dir();
        let result = load_entries(&path, true, SortField::Name, true);
        assert!(result.is_ok());
    }

    #[test]
    fn load_entries_invalid_path() {
        let path = PathBuf::from("/nonexistent/path/xyz");
        let result = load_entries(&path, true, SortField::Name, true);
        assert!(result.is_err());
    }

    #[test]
    fn dirs_sorted_before_files() {
        use std::fs;
        let tmp = std::env::temp_dir().join("rusty_adb_test_sort");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("aaa_file.txt"), b"x").unwrap();
        fs::create_dir_all(tmp.join("zzz_dir")).unwrap();

        let entries = load_entries(&tmp, false, SortField::Name, true).unwrap();
        assert!(entries[0].is_dir, "first entry should be a directory");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn hidden_files_filtered() {
        use std::fs;
        let tmp = std::env::temp_dir().join("rusty_adb_test_hidden");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join(".hidden"), b"x").unwrap();
        fs::write(tmp.join("visible.txt"), b"x").unwrap();

        let without = load_entries(&tmp, false, SortField::Name, true).unwrap();
        let with = load_entries(&tmp, true, SortField::Name, true).unwrap();

        assert_eq!(without.len(), 1);
        assert_eq!(with.len(), 2);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_entry_size_display_dir() {
        let entry = FileEntry {
            name: "foo".to_string(),
            path: PathBuf::from("/tmp/foo"),
            size: 4096,
            modified: None,
            is_dir: true,
            is_hidden: false,
        };
        assert_eq!(entry.size_display(), "--");
    }

    #[test]
    fn file_entry_size_display_file() {
        let entry = FileEntry {
            name: "bar.txt".to_string(),
            path: PathBuf::from("/tmp/bar.txt"),
            size: 2048,
            modified: None,
            is_dir: false,
            is_hidden: false,
        };
        assert_eq!(entry.size_display(), "2 KB");
    }

    // ── LocalPane state method tests ──────────────────────────────────────────

    #[test]
    fn local_pane_new_starts_at_given_path() {
        let path = std::env::temp_dir();
        let pane = LocalPane::new(path.clone());
        assert_eq!(pane.current_path, path);
        assert!(pane.error.is_none(), "should have no error on temp dir");
        assert!(pane.selected.is_empty(), "no selection on new pane");
        assert!(!pane.show_hidden);
        assert!(pane.sort_ascending, "default sort should be ascending");
    }

    #[test]
    fn local_pane_new_invalid_path_sets_error() {
        let pane = LocalPane::new(PathBuf::from("/this/path/does/not/exist/xyz"));
        assert!(pane.error.is_some());
        assert!(pane.entries.is_empty());
    }

    #[test]
    fn navigate_to_updates_path() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp.clone());
        let new_path = tmp.join("..").canonicalize().unwrap_or(tmp.clone());
        pane.navigate_to(new_path.clone());
        assert_eq!(pane.current_path, new_path);
        assert!(
            pane.selected.is_empty(),
            "selection should clear on navigation"
        );
    }

    #[test]
    fn toggle_hidden_flips_flag_and_reloads() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        assert!(!pane.show_hidden);
        pane.toggle_hidden();
        assert!(pane.show_hidden);
        pane.toggle_hidden();
        assert!(!pane.show_hidden);
    }

    #[test]
    fn sort_by_new_field_sets_ascending() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        pane.sort_by(SortField::Size);
        assert_eq!(pane.sort_by, SortField::Size);
        assert!(pane.sort_ascending);
    }

    #[test]
    fn sort_by_same_field_toggles_direction() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        assert_eq!(pane.sort_by, SortField::Name);
        assert!(pane.sort_ascending);
        pane.sort_by(SortField::Name); // click same field → descending
        assert_eq!(pane.sort_by, SortField::Name);
        assert!(!pane.sort_ascending);
        pane.sort_by(SortField::Name); // click again → ascending
        assert!(pane.sort_ascending);
    }

    #[test]
    fn sort_by_modified_reloads() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        pane.sort_by(SortField::Modified);
        assert_eq!(pane.sort_by, SortField::Modified);
        assert!(pane.error.is_none());
    }

    #[test]
    fn sort_descending_reverses_order() {
        use std::fs;
        let tmp = std::env::temp_dir().join("rusty_adb_test_desc");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("aaa.txt"), b"a").unwrap();
        fs::write(tmp.join("zzz.txt"), b"z").unwrap();

        let asc = load_entries(&tmp, false, SortField::Name, true).unwrap();
        let desc = load_entries(&tmp, false, SortField::Name, false).unwrap();

        assert_eq!(asc[0].name, "aaa.txt");
        assert_eq!(desc[0].name, "zzz.txt");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn select_toggles_entry() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        if !pane.entries.is_empty() {
            pane.select(0);
            assert!(
                pane.selected.contains(&0),
                "should be selected after first click"
            );
            pane.select(0);
            assert!(
                !pane.selected.contains(&0),
                "should be deselected after second click"
            );
        }
    }

    #[test]
    fn select_multi_adds_both() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        if pane.entries.len() >= 2 {
            pane.select(0);
            pane.select(1);
            assert_eq!(pane.selected.len(), 2);
        }
    }

    #[test]
    fn select_out_of_bounds_is_ignored() {
        let tmp = std::env::temp_dir();
        let mut pane = LocalPane::new(tmp);
        pane.select(usize::MAX);
        assert!(pane.selected.is_empty());
    }
}
