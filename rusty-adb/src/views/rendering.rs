//! Grid, icon, and column rendering implementations for file browser panes.

use crate::adb::AdbClient;
use crate::file_pane::RenameCbs;
use crate::fs::AndroidContext;
use crate::{App, Message};
use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{Border, Element, Fill, Theme};
use std::path::PathBuf;

impl App {
    /// Render local entries as 4-column compact tiles (Grid view).
    pub(super) fn view_local_grid(&self) -> Element<Message> {
        self.view_grid_impl(false)
    }

    /// Render android entries as 4-column compact tiles (Grid view).
    pub(super) fn view_android_grid(&self) -> Element<Message> {
        self.view_grid_impl(true)
    }

    /// Render local entries as 2-column large tiles (Icon view).
    pub(super) fn view_local_icon(&self) -> Element<Message> {
        self.view_icon_impl(false)
    }

    /// Render android entries as 2-column large tiles (Icon view).
    pub(super) fn view_android_icon(&self) -> Element<Message> {
        self.view_icon_impl(true)
    }

    /// Finder "As Gallery": large preview at top, horizontal filmstrip of all entries at bottom.
    pub(super) fn view_grid_impl(&self, is_android: bool) -> Element<Message> {
        const STRIP_TILE_W: u16 = 90;
        const STRIP_TILE_H: u16 = 80;
        let t = self.theme;

        // tuple: (orig_idx, name, can_navigate, is_hidden)
        // can_navigate = is_dir OR is_symlink (Android /sdcard etc. are symlinks)
        let visible: Vec<(usize, String, bool, bool)> = if is_android {
            self.android_pane
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| self.android_pane.show_hidden || !e.is_hidden)
                .map(|(i, e)| (i, e.name.clone(), e.is_navigable(), e.is_hidden))
                .collect()
        } else {
            self.local_pane
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| self.local_pane.show_hidden || !e.is_hidden)
                .map(|(i, e)| (i, e.name.clone(), e.is_navigable(), e.is_hidden))
                .collect()
        };

        let current_path = if is_android {
            self.android_pane.current_path.clone()
        } else {
            self.local_pane.current_path.clone()
        };

        let gallery_idx = if is_android {
            self.android_gallery_idx
        } else {
            self.local_gallery_idx
        }
        .min(visible.len().saturating_sub(1));

        if visible.is_empty() {
            return container(
                text("This directory is empty")
                    .size(11)
                    .color(t.text_secondary),
            )
            .padding([20, 20])
            .width(Fill)
            .height(Fill)
            .into();
        }

        // ── Large preview pane ────────────────────────────────────────────────
        let (_, preview_name, preview_is_dir, _) = visible[gallery_idx].clone();
        let preview_fg = if preview_is_dir {
            t.accent
        } else {
            t.text_secondary
        };

        let ext = preview_name
            .rsplit('.')
            .next()
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        let is_image = !preview_is_dir
            && !is_android
            && matches!(
                ext.as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
            );

        let large_preview: Element<Message> = if is_image {
            image(image::Handle::from_path(
                current_path.join(preview_name.as_str()),
            ))
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            let big_art = if preview_is_dir {
                "╔═══════════╗\n║           ║\n║     /     ║\n║           ║\n╚═══════════╝"
            } else {
                "╔═══════════╗\n║           ║\n║    ───    ║\n║           ║\n╚═══════════╝"
            };
            column![
                text(big_art)
                    .size(14)
                    .font(iced::Font::MONOSPACE)
                    .color(preview_fg),
                text(preview_name.clone())
                    .size(13)
                    .font(iced::Font::MONOSPACE)
                    .color(t.text),
            ]
            .spacing(12)
            .align_x(iced::Alignment::Center)
            .into()
        };

        let preview_area = container(large_preview)
            .width(Fill)
            .height(Fill)
            .padding(20)
            .style(move |_th| container::Style {
                background: Some(t.background_secondary.into()),
                ..Default::default()
            });

        // ── Filmstrip at bottom ───────────────────────────────────────────────
        let mut strip_tiles: Vec<Element<Message>> = Vec::new();
        for (strip_pos, &(orig_idx, ref name, is_dir, is_hidden)) in visible.iter().enumerate() {
            let is_focused = strip_pos == gallery_idx;
            let icon_fg = if is_dir { t.accent } else { t.text_secondary };
            let name_fg = if is_hidden { t.text_secondary } else { t.text };
            let short = if name.len() > 11 {
                format!("{}…", &name[..8])
            } else {
                name.clone()
            };
            let ext2 = name
                .rsplit('.')
                .next()
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            let is_img = !is_dir
                && !is_android
                && matches!(
                    ext2.as_str(),
                    "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
                );

            let thumb: Element<Message> = if is_img {
                image(image::Handle::from_path(current_path.join(name.as_str())))
                    .width(STRIP_TILE_W - 8)
                    .height(STRIP_TILE_H - 22)
                    .into()
            } else {
                let art = if is_dir {
                    "┌──┐\n│/ │\n└──┘"
                } else {
                    "┌──┐\n│──│\n└──┘"
                };
                text(art)
                    .size(9)
                    .font(iced::Font::MONOSPACE)
                    .color(icon_fg)
                    .into()
            };

            let gallery_sel_msg = if is_android {
                Message::AndroidGallerySelect(strip_pos)
            } else {
                Message::LocalGallerySelect(strip_pos)
            };
            let nav_msg = if is_android {
                Message::AndroidNavigateTo(current_path.join(name.as_str()))
            } else {
                Message::LocalNavigateTo(current_path.join(name.as_str()))
            };
            let sel_msg = if is_android {
                Message::AndroidSelectEntry(orig_idx)
            } else {
                Message::LocalSelectEntry(orig_idx)
            };
            let focused_clone = gallery_sel_msg.clone();

            let tile = container(
                button(
                    column![
                        thumb,
                        text(short)
                            .size(9)
                            .font(iced::Font::MONOSPACE)
                            .color(name_fg),
                    ]
                    .spacing(2)
                    .width(Fill)
                    .align_x(iced::Alignment::Center),
                )
                .width(Fill)
                .height(Fill)
                .padding(3)
                .on_press({
                    // Single click → focus in gallery; if dir double-click not needed,
                    // let navigation happen via the "open" button in the preview pane.
                    // Here: click focuses, and if already focused + dir → navigate.
                    if is_focused && is_dir {
                        nav_msg
                    } else if is_focused {
                        sel_msg
                    } else {
                        focused_clone
                    }
                })
                .style(t.transparent_button()),
            )
            .width(STRIP_TILE_W)
            .height(STRIP_TILE_H)
            .padding(4)
            .style(move |_t: &Theme| container::Style {
                background: if is_focused {
                    Some(t.accent.scale_alpha(0.2).into())
                } else {
                    None
                },
                border: Border {
                    color: if is_focused { t.accent } else { t.border },
                    width: if is_focused { 2.0 } else { 1.0 },
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

            strip_tiles.push(tile.into());
        }

        let filmstrip = scrollable(
            iced::widget::Row::from_vec(strip_tiles)
                .spacing(6)
                .padding([4, 8]),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .width(Fill);

        let filmstrip_container = container(filmstrip)
            .width(Fill)
            .height(STRIP_TILE_H + 16)
            .style(t.primary_panel());

        column![preview_area, filmstrip_container]
            .width(Fill)
            .height(Fill)
            .into()
    }

    pub(super) fn view_icon_impl(&self, is_android: bool) -> Element<Message> {
        // Finder "As Icons": 3-line icon art + filename below, 4 per row.
        // Layout: one flat button IS the tile — avoids nested height overflow.
        //   icon art  ≈ 3 × 15px = 45px
        //   spacing   = 5px
        //   filename  ≈ 13px
        //   btn pad   = 8px × 2 = 16px
        //   ──────────────────  79px → TILE_H = 90
        const COLS: usize = 4;
        const TILE_W: u16 = 130;
        const TILE_H: u16 = 90;
        let t = self.theme;

        // tuple: (orig_idx, name, can_navigate, is_selected, is_hidden)
        // can_navigate includes symlinks for Android (e.g. /sdcard at root)
        let visible: Vec<(usize, String, bool, bool, bool)> = if is_android {
            self.android_pane
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| self.android_pane.show_hidden || !e.is_hidden)
                .map(|(i, e)| {
                    (
                        i,
                        e.name.clone(),
                        e.is_navigable(),
                        self.android_pane.selected.contains(&i),
                        e.is_hidden,
                    )
                })
                .collect()
        } else {
            self.local_pane
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| self.local_pane.show_hidden || !e.is_hidden)
                .map(|(i, e)| {
                    (
                        i,
                        e.name.clone(),
                        e.is_navigable(),
                        self.local_pane.selected.contains(&i),
                        e.is_hidden,
                    )
                })
                .collect()
        };

        let current_path = if is_android {
            self.android_pane.current_path.clone()
        } else {
            self.local_pane.current_path.clone()
        };

        let mut col: iced::widget::Column<Message> = column![].spacing(10).padding([10, 10]);

        if visible.is_empty() {
            col = col.push(
                container(
                    text("This directory is empty")
                        .size(11)
                        .color(t.text_secondary),
                )
                .padding([8, 12]),
            );
        } else {
            for chunk in visible.chunks(COLS) {
                let mut tile_row: Vec<Element<Message>> = Vec::new();
                for &(orig_idx, ref name, can_navigate, is_selected, is_hidden) in chunk {
                    let bg: Option<iced::Background> = if is_selected {
                        Some(t.accent.scale_alpha(0.20).into())
                    } else {
                        None
                    };
                    let border_color = if is_selected { t.accent } else { t.border };
                    let border_width = if is_selected { 2.0 } else { 1.0 };
                    let icon_fg = if can_navigate {
                        t.accent
                    } else {
                        t.text_secondary
                    };
                    let name_fg = if is_hidden { t.text_secondary } else { t.text };

                    let display_name = if name.len() > 16 {
                        format!("{}…", &name[..13])
                    } else {
                        name.clone()
                    };

                    let nav_msg = if is_android {
                        Message::AndroidNavigateTo(current_path.join(name.as_str()))
                    } else {
                        Message::LocalNavigateTo(current_path.join(name.as_str()))
                    };
                    let sel_msg = if is_android {
                        Message::AndroidSelectEntry(orig_idx)
                    } else {
                        Message::LocalSelectEntry(orig_idx)
                    };
                    let press_msg = if can_navigate { nav_msg } else { sel_msg };

                    let ext = name
                        .rsplit('.')
                        .next()
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let is_image = !can_navigate
                        && !is_android
                        && matches!(
                            ext.as_str(),
                            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
                        );

                    // 3-line icon art — compact enough to leave room for the filename
                    let icon_art: Element<Message> = if is_image {
                        image(image::Handle::from_path(current_path.join(name.as_str())))
                            .width(56)
                            .height(44)
                            .into()
                    } else {
                        let art = if can_navigate {
                            "┌───┐\n│ / │\n└───┘"
                        } else {
                            "┌───┐\n│───│\n└───┘"
                        };
                        text(art)
                            .size(13)
                            .font(iced::Font::MONOSPACE)
                            .color(icon_fg)
                            .into()
                    };

                    // The button IS the tile: icon + name, no nested containers
                    let tile = button(
                        column![
                            icon_art,
                            text(display_name)
                                .size(11)
                                .font(iced::Font::MONOSPACE)
                                .color(name_fg),
                        ]
                        .spacing(5)
                        .width(Fill)
                        .align_x(iced::Alignment::Center),
                    )
                    .width(TILE_W)
                    .height(TILE_H)
                    .padding(8)
                    .on_press(press_msg)
                    .style(move |_t, _s| button::Style {
                        background: bg,
                        border: Border {
                            color: border_color,
                            width: border_width,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    });
                    tile_row.push(tile.into());
                }
                while tile_row.len() < COLS {
                    tile_row.push(iced::widget::Space::new(TILE_W, TILE_H).into());
                }
                col = col.push(
                    iced::widget::Row::from_vec(tile_row)
                        .spacing(10)
                        .padding([0, 2]),
                );
            }
        }

        scrollable(col.width(Fill)).height(Fill).into()
    }

    /// Finder "As Columns": path ancestors on the left, current entries on the right.
    ///
    /// Left column shows all ancestor directories of the current path as clickable
    /// entries. Right column renders the current directory as a list view.
    pub(super) fn view_columns_impl(&self, is_android: bool) -> Element<Message> {
        let t = self.theme;

        let current_path = if is_android {
            self.android_pane.current_path.clone()
        } else {
            self.local_pane.current_path.clone()
        };

        // ── Left: ancestor path hierarchy ──────────────────────────────────────
        // Collect ancestors from root → parent (reverse of .ancestors() output).
        let ancestors: Vec<std::path::PathBuf> = current_path
            .ancestors()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .skip(1) // skip root "/"
            .map(|p| p.to_path_buf())
            .collect();

        let mut ancestor_col: iced::widget::Column<Message> = column![].spacing(0);
        for anc in &ancestors {
            let label = anc
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".to_string());

            let is_current = anc == &current_path;
            let nav_target = anc.clone();
            let nav_msg = if is_android {
                Message::AndroidNavigateTo(nav_target)
            } else {
                Message::LocalNavigateTo(nav_target)
            };
            let fg = if is_current { t.accent } else { t.text };
            let bg: Option<iced::Background> = if is_current {
                Some(t.accent.scale_alpha(0.15).into())
            } else {
                None
            };
            let indent = "  ".repeat(anc.components().count().saturating_sub(1));
            let row_label = format!("{indent}{label}");

            ancestor_col = ancestor_col.push(
                button(
                    text(row_label)
                        .size(11)
                        .font(iced::Font::MONOSPACE)
                        .color(fg),
                )
                .width(Fill)
                .padding([3, 8])
                .on_press(nav_msg)
                .style(move |_t, _s| button::Style {
                    background: bg,
                    ..Default::default()
                }),
            );
        }

        let ancestor_panel = container(scrollable(ancestor_col.width(Fill)).height(Fill))
            .width(180)
            .height(Fill)
            .style(t.secondary_panel());

        // ── Right: current directory entry list ───────────────────────────────
        let default_ctx = AndroidContext {
            client: AdbClient {
                adb_path: PathBuf::new(),
            },
            serial: String::new(),
            storage_roots: Vec::new(),
        };
        let entry_list: Element<Message> = if is_android {
            let ctx = self.android_ctx.as_ref().unwrap_or(&default_ctx);
            self.android_pane.view_list(
                ctx,
                self.theme,
                Message::AndroidNavigateTo,
                Message::AndroidSelectEntry,
                Message::AndroidSortBy,
                Some(RenameCbs {
                    on_input: Box::new(Message::AndroidRenameInput),
                    on_commit: Message::AndroidRenameCommit,
                }),
            )
        } else {
            self.local_pane.view_list(
                &(),
                self.theme,
                Message::LocalNavigateTo,
                Message::LocalSelectEntry,
                Message::LocalSortBy,
                None,
            )
        };

        row![ancestor_panel, entry_list]
            .width(Fill)
            .height(Fill)
            .into()
    }
}
