//! Pane-level UI controls: menu bar and title bar for each file-browser pane.

use crate::{App, Message, ViewMode};
use iced::widget::{button, container, pick_list, row, text};
use iced::{Element, Fill};

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
        local_sel_has_files: bool,
        android_sel_has_files: bool,
        android_sel_single: bool,
        android_sel_nonempty: bool,
        has_device: bool,
        no_transfer: bool,
    ) -> Element<'a, Message> {
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
        let cmd_btn =
            move |label: &'static str, color: iced::Color, msg: Option<Message>| {
                let b = button(text(label).size(11).color(color))
                    .padding([2, 8])
                    .style(t.transparent_button());
                if let Some(m) = msg { b.on_press(m) } else { b }
            };

        // ── "Show" dropdown ─────────────────────────────────────────────────
        // Read column/hidden visibility from the appropriate pane.
        let (show_type, show_size, show_modified) = if is_android {
            (self.android_pane.show_type, self.android_pane.show_size, self.android_pane.show_modified)
        } else {
            (self.local_pane.show_type, self.local_pane.show_size, self.local_pane.show_modified)
        };

        // Build item labels — checkmark prefix indicates enabled state.
        let type_item:     &'static str = if show_type     { "✓ Type"         } else { "  Type"         };
        let size_item:     &'static str = if show_size     { "✓ Size"         } else { "  Size"         };
        let modified_item: &'static str = if show_modified { "✓ Modified"     } else { "  Modified"     };
        let hidden_item:   &'static str = if show_hidden   { "✓ Hidden files" } else { "  Hidden files" };

        let show_items: Vec<&'static str> = vec![type_item, size_item, modified_item, hidden_item];

        let show_picker = pick_list(show_items, None::<&str>, move |picked: &str| {
            match picked {
                s if s.contains("Type") =>
                    if is_android { Message::AndroidToggleType } else { Message::LocalToggleType },
                s if s.contains("Size") =>
                    if is_android { Message::AndroidToggleSize } else { Message::LocalToggleSize },
                s if s.contains("Modified") =>
                    if is_android { Message::AndroidToggleModified } else { Message::LocalToggleModified },
                _ => if is_android { Message::AndroidToggleHidden } else { Message::LocalToggleHidden },
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
                    cmd_btn("To Local", t.accent, Some(Message::CopyToLocal)).into(),
                );
            }
            if has_device && android_sel_single {
                cmd_items.push(
                    cmd_btn("Rename", t.accent, Some(Message::AndroidBeginRename)).into(),
                );
            }
            if has_device && android_sel_nonempty {
                cmd_items.push(
                    cmd_btn("Delete", t.error, Some(Message::AndroidBeginDelete)).into(),
                );
            }
        } else {
            // local pane
            if has_device && no_transfer && local_sel_has_files {
                cmd_items.push(
                    cmd_btn("To Android", t.accent, Some(Message::CopyToAndroid)).into(),
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
            .height(28)
            .style(t.secondary_panel())
            .into()
    }

    /// Box-drawing title bar rendered above each file-browser pane.
    ///
    /// Produces a single line like:  `┌─ LOCAL ─ /home/user ──`
    /// in monospace accent color on a secondary-background strip.
    pub(super) fn view_pane_title_bar<'a>(&self, label: &str, path: &str) -> Element<'a, Message> {
        let t = self.theme;
        let title = format!("┌─ {label} ─ {path}");
        container(
            text(title)
                .size(11)
                .font(iced::Font::MONOSPACE)
                .color(t.accent),
        )
        .width(Fill)
        .padding([3, 8])
        .style(t.secondary_panel())
        .into()
    }
}
