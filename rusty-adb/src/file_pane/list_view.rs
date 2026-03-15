//! List-view rendering for FilePane.

use super::{FilePane, RenameCbs, RENAME_INPUT_ID};
use crate::fs::{FileSystem, IconKind, PaneState, SortField};
use crate::icons;
use crate::theme::ThemeColors;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Border, Element, Fill};

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
    ///   do not support inline rename (local filesystem)
    pub fn view_list<'a, 'c, Message: 'a + Clone>(
        &'a self,
        ctx: &'c FS::Context,
        theme: ThemeColors,
        on_navigate: impl Fn(std::path::PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
        on_sort: impl Fn(SortField) -> Message + 'a,
        rename_cbs: Option<RenameCbs<'a, Message>>,
    ) -> Element<'a, Message> {
        // ── Column widths (shared between header and every data row) ──────────
        const W_ICON: u16 = 24;
        const W_TYPE: u16 = 52;
        const W_SIZE: u16 = 84;
        const W_DATE: u16 = 90;

        let t = theme;

        // ── Sort indicator ────────────────────────────────────────────────────
        let sort_ind = |field: SortField| -> &'static str {
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

        // ── Column header builder ─────────────────────────────────────────────
        let mk_hdr = |label: String,
                      field: SortField,
                      width: iced::Length,
                      msg: Message|
         -> Element<'a, Message> {
            let fg = if self.sort_by == field {
                t.accent
            } else {
                t.text_secondary
            };
            button(text(label).size(10).color(fg))
                .width(width)
                .padding([2, 4])
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(msg)
                .into()
        };

        // ── Sticky column header row ──────────────────────────────────────────
        let name_lbl = format!("Name{}", sort_ind(SortField::Name));
        let size_lbl = format!("Size{}", sort_ind(SortField::Size));
        let date_lbl = format!("Modified{}", sort_ind(SortField::Modified));

        let mut hdr_cells: Vec<Element<Message>> = vec![
            iced::widget::Space::new(20, 1).into(), // checkbox placeholder
            text("").width(W_ICON).into(),          // icon placeholder
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
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        // ── State guard: show spinner / error / no-device ────────────────────
        if self.state != PaneState::Ready {
            let retry = Some(on_navigate(self.current_path.clone()));
            let body = self.view_state_body(theme, retry);
            return column![col_header, body].width(Fill).height(Fill).into();
        }

        // ── Entry rows ────────────────────────────────────────────────────────
        let is_at_nav_root = FS::is_nav_root(ctx, &self.current_path);

        // Pre-build the inline rename input (consumes rename callbacks once).
        let mut rename_row: Option<(usize, Element<Message>)> =
            if let (Some(cbs), Some((idx, val))) = (rename_cbs, &self.rename_pending) {
                let icon_glyph = self
                    .entries
                    .get(*idx)
                    .map(|e| icon_str(e.icon_kind()))
                    .unwrap_or_else(icons::file);
                let input = text_input("New name…", val.as_str())
                    .id(text_input::Id::new(RENAME_INPUT_ID))
                    .on_input(cbs.on_input)
                    .on_submit(cbs.on_commit)
                    .size(12)
                    .width(Fill);
                Some((
                    *idx,
                    row![text(icon_glyph).font(icons::font()).size(12), input]
                        .spacing(6)
                        .padding([1, 0])
                        .into(),
                ))
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
                    .style(move |_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    })
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
            let bg: Option<iced::Background> = if is_selected {
                Some(t.accent.scale_alpha(0.2).into())
            } else {
                None
            };
            let fg = if entry.is_hidden {
                t.text_secondary
            } else {
                t.text
            };
            let icon_fg = if entry.is_navigable() {
                t.accent
            } else {
                t.text_secondary
            };

            let entry_path = entry.path.clone();
            let can_navigate = entry.is_navigable();
            let on_nav = on_navigate(entry_path);
            let on_sel_btn = on_select(i);
            let on_sel_chk = on_select(i);

            // If this entry is being renamed, use the pre-built rename input.
            let row_element: Element<Message> = if rename_row
                .as_ref()
                .map(|(idx, _)| *idx == i)
                .unwrap_or(false)
            {
                rename_row.take().unwrap().1
            } else {
                let mut cells: Vec<Element<Message>> = vec![
                    text(icon_str(entry.icon_kind()))
                        .font(icons::font())
                        .size(11)
                        .color(icon_fg)
                        .width(W_ICON)
                        .into(),
                    text(entry.name.clone())
                        .size(11)
                        .color(fg)
                        .width(Fill)
                        .into(),
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
                        .style(|_t, _s| button::Style {
                            background: None,
                            ..Default::default()
                        })
                        .on_press(if can_navigate {
                            on_nav
                        } else {
                            on_sel_btn
                        }),
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

    // ── Icon helpers ──────────────────────────────────────────────────────────
}

/// Map an [`IconKind`] to its Nerd Font glyph string.
fn icon_str(kind: IconKind) -> String {
    match kind {
        IconKind::Folder => icons::folder(),
        IconKind::File => icons::file(),
        IconKind::Symlink => icons::symlink(),
    }
}

impl<FS: FileSystem> FilePane<FS> {
    // ── State body helpers ────────────────────────────────────────────────────

    pub(super) fn view_state_body<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_retry: Option<Message>,
    ) -> Element<'a, Message> {
        use crate::file_pane::shared_views::{view_connect_guide, view_error};
        match &self.state {
            PaneState::NoDevice => view_connect_guide(theme),
            PaneState::Loading => self.view_loading(theme),
            PaneState::Error(e) => view_error(theme, e, on_retry),
            PaneState::Ready => unreachable!("guarded above"),
        }
    }

    pub(super) fn view_loading<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
    ) -> Element<'a, Message> {
        let spinner_chars = ["|", "/", "-", "\\"];
        let spinner = spinner_chars[(self.spinner_frame as usize) % spinner_chars.len()];

        let content = row![
            text(spinner).size(14).color(theme.accent),
            text("  Fetching directory listing…")
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
}
