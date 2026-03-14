//! Pane-level UI controls: menu bar and title bar for each file-browser pane.

use crate::icons;
use crate::{App, Message, PaneLayout, ViewMode};
use iced::widget::tooltip::Position as TipPos;
use iced::widget::{button, container, horizontal_space, pick_list, row, text, tooltip};
use iced::{Element, Fill, FillPortion};
use std::path::{Path, PathBuf};

// ─── Path Segment Helper ──────────────────────────────────────────────────────

/// Split `path` into root-first `(display_label, full_path)` segments for
/// breadcrumb rendering.
///
/// Example:
/// ```text
/// /sdcard/DCIM  →  [("/", "/"), ("sdcard", "/sdcard"), ("DCIM", "/sdcard/DCIM")]
/// ```
pub(crate) fn path_breadcrumb_segments(path: &Path) -> Vec<(String, PathBuf)> {
    let mut segments: Vec<(String, PathBuf)> = path
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

    // Ensure a root segment is always present.
    if segments.first().map(|(l, _)| l.as_str()) != Some("/") {
        segments.insert(0, ("/".to_string(), PathBuf::from("/")));
    }
    segments
}

/// Selection and device state passed to the pane menu bar.
pub(crate) struct PaneMenuState {
    pub local_sel_has_files: bool,
    pub android_sel_has_files: bool,
    pub android_sel_single: bool,
    pub android_sel_nonempty: bool,
    pub has_device: bool,
    pub no_transfer: bool,
}

impl App {
    /// Horizontal menu bar rendered at the top of each directory pane.
    ///
    /// Contains (left to right):
    /// - View mode toggle group: [List] [Details] [Grid] [Icon]
    /// - Spacer
    /// - File command buttons (pane-specific, enabled/disabled by selection)
    /// - Hidden files toggle: [.Hidden] / [.Shown]
    pub(super) fn view_pane_menu_bar<'a>(
        &'a self,
        is_android: bool,
        view_mode: ViewMode,
        show_hidden: bool,
        state: PaneMenuState,
    ) -> Element<'a, Message> {
        let PaneMenuState {
            local_sel_has_files,
            android_sel_has_files,
            android_sel_single,
            android_sel_nonempty,
            has_device,
            no_transfer,
        } = state;
        let t = self.theme;

        // ── view mode drop-down picker ──────────────────────────────────────
        let view_picker = pick_list(ViewMode::ALL, Some(view_mode), move |mode| {
            if is_android {
                Message::SetAndroidViewMode(mode)
            } else {
                Message::SetLocalViewMode(mode)
            }
        })
        .text_size(11)
        .padding([2, 8]);

        // ── file command button ─────────────────────────────────────────────
        // `icon_glyph` is an optional Nerd Font glyph string (e.g. icons::trash()).
        let cmd_btn = move |icon_glyph: Option<String>,
                            label: &'static str,
                            color: iced::Color,
                            msg: Option<Message>| {
            let content: iced::Element<Message> = if let Some(g) = icon_glyph {
                row![
                    text(g).font(icons::font()).size(13).color(color),
                    text(label).size(11).color(color),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .into()
            } else {
                text(label).size(11).color(color).into()
            };
            let b = button(content)
                .padding([2, 8])
                .style(t.transparent_button());
            if let Some(m) = msg {
                b.on_press(m)
            } else {
                b
            }
        };

        // ── "Show" dropdown ─────────────────────────────────────────────────
        // Read column/hidden visibility from the appropriate pane.
        let (show_type, show_size, show_modified) = if is_android {
            (
                self.android_pane.show_type,
                self.android_pane.show_size,
                self.android_pane.show_modified,
            )
        } else {
            (
                self.local_pane.show_type,
                self.local_pane.show_size,
                self.local_pane.show_modified,
            )
        };

        // Build item labels — checkmark prefix indicates enabled state.
        let type_item: &'static str = if show_type { "✓ Type" } else { "  Type" };
        let size_item: &'static str = if show_size { "✓ Size" } else { "  Size" };
        let modified_item: &'static str = if show_modified {
            "✓ Modified"
        } else {
            "  Modified"
        };
        let hidden_item: &'static str = if show_hidden {
            "✓ Hidden files"
        } else {
            "  Hidden files"
        };

        let show_items: Vec<&'static str> = vec![type_item, size_item, modified_item, hidden_item];

        let show_picker = pick_list(show_items, None::<&str>, move |picked: &str| match picked {
            s if s.contains("Type") => {
                if is_android {
                    Message::AndroidToggleType
                } else {
                    Message::LocalToggleType
                }
            }
            s if s.contains("Size") => {
                if is_android {
                    Message::AndroidToggleSize
                } else {
                    Message::LocalToggleSize
                }
            }
            s if s.contains("Modified") => {
                if is_android {
                    Message::AndroidToggleModified
                } else {
                    Message::LocalToggleModified
                }
            }
            _ => {
                if is_android {
                    Message::AndroidToggleHidden
                } else {
                    Message::LocalToggleHidden
                }
            }
        })
        .placeholder("Show")
        .text_size(11)
        .padding([2, 8]);

        // ── file commands ───────────────────────────────────────────────────
        let mut cmd_items: Vec<Element<Message>> = Vec::new();
        if is_android {
            if has_device && no_transfer && android_sel_has_files {
                cmd_items.push(
                    cmd_btn(
                        Some(icons::download()),
                        "To Local",
                        t.accent,
                        Some(Message::CopyToLocal),
                    )
                    .into(),
                );
            }
            if has_device && android_sel_single {
                cmd_items.push(
                    cmd_btn(None, "Rename", t.accent, Some(Message::AndroidBeginRename)).into(),
                );
            }
            if has_device && android_sel_nonempty {
                cmd_items.push(
                    cmd_btn(
                        Some(icons::trash()),
                        "Delete",
                        t.error,
                        Some(Message::AndroidBeginDelete),
                    )
                    .into(),
                );
            }
        } else {
            // local pane
            if has_device && no_transfer && local_sel_has_files {
                cmd_items.push(
                    cmd_btn(
                        Some(icons::upload()),
                        "To Android",
                        t.accent,
                        Some(Message::CopyToAndroid),
                    )
                    .into(),
                );
            }
        }

        let cmd_row = iced::widget::Row::from_vec(cmd_items)
            .spacing(4)
            .align_y(iced::Alignment::Center);

        let content = row![
            text("View").size(11).color(t.text_secondary),
            iced::widget::Space::new(6, 1),
            view_picker,
            iced::widget::Space::new(8, 1),
            text("Show").size(11).color(t.text_secondary),
            iced::widget::Space::new(6, 1),
            show_picker,
            iced::widget::horizontal_space(),
            cmd_row,
        ]
        .spacing(2)
        .padding([2, 8])
        .align_y(iced::Alignment::Center);

        container(content)
            .width(Fill)
            .height(32)
            .align_y(iced::Alignment::Center)
            .style(t.secondary_panel())
            .into()
    }

    /// Title bar rendered above each file-browser pane.
    ///
    /// Combines the former static title (`┌─ LOCAL ─ /path`) with the inner
    /// breadcrumb widget: the path is split into clickable ancestor segments
    /// while keeping the `┌─ LABEL ─` box-drawing prefix.
    ///
    /// Each ancestor segment emits `on_navigate(ancestor_path)` when clicked.
    /// The current (last) segment is non-clickable, rendered in accent color.
    ///
    /// An expand/collapse toggle button is shown on the right:
    /// - Split state → ⤢ (expand this pane)
    /// - This pane is expanded → ⤡ (restore split)
    /// - Other pane is expanded → no button (this pane is the sidebar)
    pub(super) fn view_pane_title_bar<'a>(
        &self,
        label: &str,
        current_path: &Path,
        is_android: bool,
        pane_layout: PaneLayout,
        on_navigate: impl Fn(PathBuf) -> Message,
    ) -> Element<'a, Message> {
        let t = self.theme;
        let segments = path_breadcrumb_segments(current_path);
        let last_idx = segments.len().saturating_sub(1);

        let mut cells: Vec<Element<'a, Message>> = Vec::new();

        // ┌─ LABEL ─ prefix
        cells.push(
            text(format!("┌─ {} ─", label))
                .size(11)
                .font(iced::Font::MONOSPACE)
                .color(t.accent)
                .into(),
        );

        for (i, (seg_label, path)) in segments.into_iter().enumerate() {
            let is_last = i == last_idx;
            if is_last {
                // Current directory — non-clickable, accent color
                cells.push(
                    text(seg_label)
                        .size(11)
                        .font(iced::Font::MONOSPACE)
                        .color(t.accent)
                        .into(),
                );
            } else {
                // Ancestor — clickable link
                let nav_msg = on_navigate(path);
                cells.push(
                    button(
                        text(seg_label)
                            .size(11)
                            .font(iced::Font::MONOSPACE)
                            .color(t.text_secondary),
                    )
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    })
                    .padding([0, 2])
                    .on_press(nav_msg)
                    .into(),
                );
                cells.push(
                    text("›")
                        .size(11)
                        .font(iced::Font::MONOSPACE)
                        .color(t.text_secondary)
                        .into(),
                );
            }
        }

        // ── Expand / collapse toggle — right-anchored in 1/3 column ─────────
        let this_is_expanded = (is_android && pane_layout == PaneLayout::AndroidExpanded)
            || (!is_android && pane_layout == PaneLayout::LocalExpanded);
        let other_is_expanded = (is_android && pane_layout == PaneLayout::LocalExpanded)
            || (!is_android && pane_layout == PaneLayout::AndroidExpanded);

        // Breadcrumbs occupy 2/3, action button occupies 1/3 (right-aligned).
        let breadcrumbs = container(
            iced::widget::Row::from_vec(cells)
                .spacing(2)
                .align_y(iced::Alignment::Center),
        )
        .width(FillPortion(2));

        let action: Element<'a, Message> = if other_is_expanded {
            horizontal_space().into()
        } else {
            let (icon, tip, msg) = if this_is_expanded {
                (
                    icons::collapse(),
                    "Restore equal split",
                    Message::CollapsePanes,
                )
            } else {
                (
                    icons::expand(),
                    "Expand this pane",
                    Message::ExpandPane(is_android),
                )
            };
            let btn = button(
                text(icon)
                    .size(14)
                    .color(t.text_secondary)
                    .font(icons::font()),
            )
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .padding([0, 4])
            .on_press(msg);
            container(tooltip(btn, text(tip).size(11), TipPos::Bottom))
                .width(FillPortion(1))
                .align_x(iced::Alignment::End)
                .into()
        };

        container(iced::widget::row![breadcrumbs, action].align_y(iced::Alignment::Center))
            .width(Fill)
            .padding([6, 8])
            .style(t.secondary_panel())
            .into()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_root_path() {
        let segs = path_breadcrumb_segments(Path::new("/"));
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].0, "/");
        assert_eq!(segs[0].1, PathBuf::from("/"));
    }

    #[test]
    fn segments_single_level() {
        let segs = path_breadcrumb_segments(Path::new("/sdcard"));
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].0, "/");
        assert_eq!(segs[0].1, PathBuf::from("/"));
        assert_eq!(segs[1].0, "sdcard");
        assert_eq!(segs[1].1, PathBuf::from("/sdcard"));
    }

    #[test]
    fn segments_nested_path() {
        let segs = path_breadcrumb_segments(Path::new("/sdcard/DCIM/Camera"));
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].0, "/");
        assert_eq!(segs[1].0, "sdcard");
        assert_eq!(segs[2].0, "DCIM");
        assert_eq!(segs[3].0, "Camera");
        assert_eq!(segs[3].1, PathBuf::from("/sdcard/DCIM/Camera"));
    }

    #[test]
    fn segments_root_always_present_on_relative_path() {
        // Even if given a relative path, a root "/" segment is prepended.
        let segs = path_breadcrumb_segments(Path::new("foo/bar"));
        assert_eq!(segs[0].0, "/");
    }

    #[test]
    fn view_pane_title_bar_renders() {
        // Smoke test: rendering must not panic for all layout states.
        let app = crate::App::default();
        for layout in [
            crate::PaneLayout::Split,
            crate::PaneLayout::LocalExpanded,
            crate::PaneLayout::AndroidExpanded,
        ] {
            let _ = app.view_pane_title_bar(
                "LOCAL",
                Path::new("/Users/aaron"),
                false,
                layout,
                crate::Message::LocalNavigateTo,
            );
        }
    }

    // ── view_pane_menu_bar smoke tests ─────────────────────────────────────

    fn default_state() -> PaneMenuState {
        PaneMenuState {
            local_sel_has_files: false,
            android_sel_has_files: false,
            android_sel_single: false,
            android_sel_nonempty: false,
            has_device: false,
            no_transfer: true,
        }
    }

    #[test]
    fn view_pane_menu_bar_local_no_selection_renders() {
        let app = crate::App::default();
        let _ = app.view_pane_menu_bar(false, crate::ViewMode::List, false, default_state());
    }

    #[test]
    fn view_pane_menu_bar_android_no_selection_renders() {
        let app = crate::App::default();
        let _ = app.view_pane_menu_bar(true, crate::ViewMode::List, false, default_state());
    }

    #[test]
    fn view_pane_menu_bar_android_with_device_and_selection_renders() {
        let app = crate::App::default();
        let state = PaneMenuState {
            local_sel_has_files: false,
            android_sel_has_files: true,
            android_sel_single: true,
            android_sel_nonempty: true,
            has_device: true,
            no_transfer: true,
        };
        let _ = app.view_pane_menu_bar(true, crate::ViewMode::Details, false, state);
    }

    #[test]
    fn view_pane_menu_bar_local_with_device_and_files_selected_renders() {
        let app = crate::App::default();
        let state = PaneMenuState {
            local_sel_has_files: true,
            android_sel_has_files: false,
            android_sel_single: false,
            android_sel_nonempty: false,
            has_device: true,
            no_transfer: true,
        };
        let _ = app.view_pane_menu_bar(false, crate::ViewMode::Grid, true, state);
    }

    #[test]
    fn view_pane_menu_bar_all_view_modes_render() {
        let app = crate::App::default();
        for mode in [
            crate::ViewMode::List,
            crate::ViewMode::Details,
            crate::ViewMode::Grid,
            crate::ViewMode::Icon,
        ] {
            let _ = app.view_pane_menu_bar(false, mode, false, default_state());
        }
    }

    // ── shared_views smoke tests ───────────────────────────────────────────

    #[test]
    fn view_error_without_retry_renders() {
        use crate::file_pane::view_error;
        let theme = crate::App::default().theme;
        let _: iced::Element<crate::Message> = view_error(theme, "directory not found", None);
    }

    #[test]
    fn view_error_with_retry_renders() {
        use crate::file_pane::view_error;
        let theme = crate::App::default().theme;
        let _: iced::Element<crate::Message> =
            view_error(theme, "timeout", Some(crate::Message::RefreshPanes));
    }

    #[test]
    fn view_connect_guide_renders() {
        use crate::file_pane::view_connect_guide;
        let theme = crate::App::default().theme;
        let _: iced::Element<crate::Message> = view_connect_guide(theme);
    }
}
