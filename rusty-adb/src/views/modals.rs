//! Modal overlay views: preview, log viewer, about, and settings.

use crate::adb::AdbStatus;
use crate::queue::QueueStatus;
use crate::{App, LogLevel, Message, PreviewContent};
use iced::widget::{
    button, column, container, image, pick_list, progress_bar, row, scrollable, text, text_input,
    toggler, Row,
};
use iced::{Border, Element, Fill};

impl App {
    /// The semi-transparent backdrop captures clicks (closing the modal).
    /// The inner card shows either an image or scrollable text.
    pub(super) fn view_preview_modal<'a>(
        &'a self,
        content: &'a PreviewContent,
    ) -> Element<'a, Message> {
        let t = self.theme;

        let close_btn = button(text("X  Close").size(12).color(t.text))
            .style(t.secondary_button())
            .padding([4, 12])
            .on_press(Message::ClosePreview);

        let preview_body: Element<Message> = match content {
            PreviewContent::Image(path) => {
                let handle = image::Handle::from_path(path);
                container(image(handle).width(Fill).height(Fill))
                    .width(Fill)
                    .height(Fill)
                    .into()
            }
            PreviewContent::Text(text_content) => scrollable(
                container(
                    text(text_content.clone())
                        .size(12)
                        .color(t.text)
                        .font(iced::Font::with_name("Menlo")),
                )
                .padding([8, 12]),
            )
            .width(Fill)
            .height(Fill)
            .into(),
            PreviewContent::Unsupported(msg) => {
                container(text(msg.clone()).size(12).color(t.text_secondary))
                    .width(Fill)
                    .height(Fill)
                    .center_x(Fill)
                    .center_y(Fill)
                    .into()
            }
        };

        let card = container(
            column![
                // Header bar with close button
                container(
                    row![
                        text("Preview").size(13).color(t.text).width(Fill),
                        close_btn,
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(8)
                    .padding([4, 8]),
                )
                .width(Fill)
                .style(t.secondary_panel()),
                // Content area
                container(preview_body)
                    .width(Fill)
                    .height(Fill)
                    .padding(8)
                    .style(move |_th| container::Style {
                        background: Some(t.background.into()),
                        ..Default::default()
                    }),
            ]
            .width(Fill)
            .height(Fill),
        )
        .width(700)
        .height(500)
        .style(t.primary_panel());

        // Semi-transparent backdrop — fills the full window
        super::modal_backdrop(card)
    }

    /// Log viewer modal overlay.
    ///
    /// Shows a file picker (drop-down), a level filter, and the log content.
    pub(super) fn view_log_viewer(&self) -> Element<Message> {
        let t = self.theme;

        // ── File picker ────────────────────────────────────────────────────
        let file_names: Vec<String> = self
            .log_viewer_files
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("(unknown)")
                    .to_string()
            })
            .collect();

        let selected_name: Option<String> = self.log_viewer_selected.as_ref().and_then(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        });

        // Build the file pick_list — map chosen name back to path on selection
        let files_for_closure = self.log_viewer_files.clone();
        let file_picker = pick_list(file_names, selected_name, move |chosen: String| {
            let path = files_for_closure
                .iter()
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == chosen)
                        .unwrap_or(false)
                })
                .cloned()
                .unwrap_or_default();
            Message::LogViewerSelectFile(path)
        })
        .text_size(11)
        .padding([2, 8]);

        // ── Level filter ───────────────────────────────────────────────────
        let level_picker = pick_list(
            LogLevel::ALL,
            Some(self.log_viewer_level),
            Message::LogViewerSetLevel,
        )
        .text_size(11)
        .padding([2, 8]);

        let log_scroll = scrollable(
            container(
                text(self.log_viewer_display.clone())
                    .size(11)
                    .font(iced::Font::MONOSPACE)
                    .color(t.text),
            )
            .padding(8)
            .width(Fill),
        )
        .height(Fill);

        // ── Toolbar row ───────────────────────────────────────────────────
        let toolbar = row![
            text("File:").size(11).color(t.text_secondary),
            file_picker,
            iced::widget::horizontal_space(),
            text("Level:").size(11).color(t.text_secondary),
            level_picker,
            button(text("Close").size(11).color(t.text))
                .style(t.secondary_button())
                .padding([2, 10])
                .on_press(Message::CloseLogViewer),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .padding([6, 10]);

        let card = container(
            column![
                // Header
                container(
                    row![text("Log Viewer").size(14).color(t.text).width(Fill),].padding([6, 10]),
                )
                .width(Fill)
                .style(t.secondary_panel()),
                // Toolbar
                container(toolbar)
                    .width(Fill)
                    .style(move |_th| container::Style {
                        background: Some(t.background_secondary.scale_alpha(0.5).into()),
                        border: Border {
                            color: t.border,
                            width: 1.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                // Log content
                log_scroll,
            ]
            .width(Fill)
            .height(Fill),
        )
        .width(iced::Length::FillPortion(9))
        .height(iced::Length::FillPortion(8))
        .max_width(1100.0)
        .max_height(700.0)
        .style(move |_th| container::Style {
            background: Some(t.background.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: iced::Shadow {
                color: iced::Color::BLACK.scale_alpha(0.5),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 24.0,
            },
            ..Default::default()
        });

        super::modal_backdrop(card)
    }

    /// About modal overlay — app info, version, GitHub link, license.
    pub(super) fn view_about_modal(&self) -> Element<Message> {
        let t = self.theme;
        let version = env!("CARGO_PKG_VERSION");

        let close_btn = button(text("X  Close").size(12).color(t.text))
            .style(t.secondary_button())
            .padding([4, 12])
            .on_press(Message::CloseAbout);

        let github_btn = button(
            text("github.com/aaroncroberts/rusty-adb")
                .size(12)
                .color(t.accent),
        )
        .style(t.transparent_button())
        .on_press(Message::OpenUrl(
            "https://github.com/aaroncroberts/rusty-adb".to_string(),
        ));

        let card = container(
            column![
                // Header
                container(
                    row![
                        text("About rusty-adb").size(14).color(t.text).width(Fill),
                        close_btn,
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(8)
                    .padding([6, 10]),
                )
                .width(Fill)
                .style(t.secondary_panel()),
                // Body
                container(
                    column![
                        // ASCII art logo — top/bottom border only, centered
                        container(
                            column![
                                text("══════════════════════════════════════════════")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                                text(" _ __ _   _ ___| |_ _   _    __ _  __| | |__ ")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                                text("| '__| | | / __| __| | | |  / _` |/ _` | '_ \\")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                                text("| |  | |_| \\__ \\ |_| |_| | | (_| | (_| | |_) |")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                                text("|_|   \\__,_|___/\\__|\\__, |  \\__,_|\\__,_|_.__/ ")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                                text("                     |___/                     ")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                                text("").size(6),
                                text(format!("  Android file manager  v{version:<21}"))
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.text),
                                text("══════════════════════════════════════════════")
                                    .size(12)
                                    .font(iced::Font::MONOSPACE)
                                    .color(t.accent),
                            ]
                            .spacing(0),
                        )
                        .width(Fill)
                        .align_x(iced::Alignment::Center),
                        text("").size(8), // spacer
                        text("Android file manager built with Rust and Iced.")
                            .size(12)
                            .color(t.text_secondary),
                        text("").size(4), // spacer
                        github_btn,
                        text("License: MIT").size(11).color(t.text_secondary),
                    ]
                    .spacing(6),
                )
                .padding([20, 20])
                .width(Fill),
            ]
            .width(Fill),
        )
        .width(480)
        .style(t.primary_panel());

        super::modal_backdrop(card)
    }

    /// Settings modal overlay — rendered on top of the full UI via `stack!`.
    pub(super) fn view_settings_modal(&self) -> Element<Message> {
        let t = self.theme;

        const LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];
        let selected_level: Option<&'static str> = LOG_LEVELS
            .iter()
            .copied()
            .find(|&l| l == self.settings_draft.log.level);

        let level_row: Element<Message> = row![
            text("Log level:").size(12).color(t.text).width(120),
            pick_list(LOG_LEVELS, selected_level, |l: &'static str| {
                Message::SettingsDraftLogLevel(l.to_string())
            },)
            .text_size(12),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12)
        .into();

        let console_row: Element<Message> = row![
            text("Console log:").size(12).color(t.text).width(120),
            toggler(self.settings_draft.log.console_enabled)
                .on_toggle(Message::SettingsDraftConsole),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12)
        .into();

        let file_row: Element<Message> = row![
            text("File log:").size(12).color(t.text).width(120),
            toggler(self.settings_draft.log.file_enabled).on_toggle(Message::SettingsDraftFile),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(12)
        .into();

        let open_folder_btn = button(text("Open Log Folder").size(12).color(t.accent))
            .style(t.transparent_button())
            .on_press(Message::OpenLogFolder);

        let view_logs_btn = button(text("View Logs").size(12).color(t.accent))
            .style(t.transparent_button())
            .on_press(Message::OpenLogViewer);

        let save_btn = button(text("Save").size(12).color(iced::Color::WHITE))
            .style(t.accent_button())
            .padding([5, 16])
            .on_press(Message::SaveSettings);

        let cancel_btn = button(text("Cancel").size(12).color(t.text))
            .style(t.secondary_button())
            .padding([5, 12])
            .on_press(Message::CloseSettings);

        let card = container(
            column![
                // Header
                container(
                    row![
                        text("Settings").size(14).color(t.text).width(Fill),
                        cancel_btn,
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(8)
                    .padding([6, 10]),
                )
                .width(Fill)
                .style(t.secondary_panel()),
                // Body
                container(
                    column![
                        level_row,
                        console_row,
                        file_row,
                        row![open_folder_btn, view_logs_btn].spacing(8),
                        text("Changes apply on next launch.")
                            .size(11)
                            .color(t.text_secondary),
                        // ── Divider ─────────────────────────────────────────
                        container(iced::widget::horizontal_rule(1))
                            .padding([4, 0])
                            .width(Fill),
                        // ── Device Actions ───────────────────────────────────
                        text("Device Actions").size(11).color(t.text_secondary),
                        {
                            let has_device = self.active_serial.is_some();
                            let daemon_error = matches!(&self.adb_status, AdbStatus::Error(_));
                            let action_btn =
                                move |label: &str, color: iced::Color, msg: Option<Message>| {
                                    let b = button(text(label.to_string()).size(12).color(color))
                                        .style(t.secondary_button())
                                        .padding([5, 14]);
                                    let elem: Element<Message> = if let Some(m) = msg {
                                        b.on_press(m).into()
                                    } else {
                                        b.into()
                                    };
                                    elem
                                };
                            let refresh =
                                action_btn("Refresh", t.text, Some(Message::RefreshPanes));
                            let disconnect = action_btn(
                                "Disconnect",
                                if has_device {
                                    t.warning
                                } else {
                                    t.text_secondary
                                },
                                if has_device {
                                    Some(Message::DisconnectDevice)
                                } else {
                                    None
                                },
                            );
                            let restart = action_btn(
                                "Restart Daemon",
                                if daemon_error { t.warning } else { t.text },
                                Some(Message::RestartDaemon),
                            );
                            let r: Element<Message> =
                                Row::from_vec(vec![refresh, disconnect, restart])
                                    .spacing(8)
                                    .into();
                            r
                        },
                    ]
                    .spacing(14),
                )
                .padding([16, 16])
                .width(Fill),
                // Footer
                container(
                    row![iced::widget::Space::with_width(Fill), save_btn,]
                        .spacing(8)
                        .padding([8, 12]),
                )
                .width(Fill)
                .style(t.secondary_panel()),
            ]
            .width(Fill),
        )
        .width(420)
        .style(t.primary_panel());

        super::modal_backdrop(card)
    }

    // ── Copy Confirm Dialog ───────────────────────────────────────────────────

    /// Confirmation dialog before enqueuing local files for copy to Android.
    ///
    /// Shows the list of selected files, the destination (current android path),
    /// and Change / Confirm buttons.
    pub(super) fn view_copy_confirm(&self) -> Element<Message> {
        let t = self.theme;

        // Selected file names
        let selected: Vec<&str> = self
            .local_pane
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| self.local_pane.selected.contains(i))
            .map(|(_, e)| e.name.as_str())
            .collect();

        let count = selected.len();
        let dest = self.android_pane.current_path.display().to_string();

        // File list — scrollable when many items selected
        let file_rows: Vec<Element<Message>> = selected
            .iter()
            .map(|name| {
                text(format!("  • {name}"))
                    .size(11)
                    .color(t.text)
                    .into()
            })
            .collect();

        let file_list = scrollable(column(file_rows).spacing(2).padding([4, 0]))
            .height(iced::Length::Fixed(180.0));

        let dest_label = row![
            text("Destination: ").size(11).color(t.text_secondary),
            text(dest.clone())
                .size(11)
                .color(t.accent)
                .font(iced::Font::MONOSPACE),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);

        let change_btn = button(text("Change destination").size(11).color(t.text_secondary))
            .style(t.transparent_button())
            .padding([4, 12])
            .on_press(Message::CloseCopyConfirm);

        let confirm_btn = button(
            text(format!("Queue {count} item{}", if count == 1 { "" } else { "s" }))
                .size(12)
                .color(iced::Color::WHITE),
        )
        .style(t.accent_button())
        .padding([4, 16])
        .on_press(Message::ConfirmCopyToDevice);

        let footer = container(
            row![
                text("").width(Fill), // spacer
                change_btn,
                confirm_btn,
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .padding([6, 10]),
        )
        .width(Fill)
        .style(t.secondary_panel());

        let body = container(
            column![
                text(format!("{count} item{} selected:", if count == 1 { "" } else { "s" }))
                    .size(11)
                    .color(t.text_secondary),
                file_list,
                iced::widget::Space::new(Fill, 8),
                dest_label,
                text("Click 'Change destination' to return and pick a different folder.")
                    .size(10)
                    .color(t.text_secondary),
            ]
            .spacing(6)
            .padding([10, 14])
            .width(Fill),
        )
        .width(Fill);

        let card = container(
            column![
                container(
                    row![
                        text("Copy to Device").size(13).color(t.text).width(Fill),
                        button(text("X").size(11).color(t.text_secondary))
                            .style(t.transparent_button())
                            .padding([2, 6])
                            .on_press(Message::CloseCopyConfirm),
                    ]
                    .padding([6, 10])
                    .align_y(iced::Alignment::Center),
                )
                .width(Fill)
                .style(t.secondary_panel()),
                body,
                footer,
            ]
            .width(Fill),
        )
        .width(460)
        .style(t.primary_panel());

        super::modal_backdrop(card)
    }

    // ── Queue Management Dialog ───────────────────────────────────────────────

    /// Queue management dialog: shows all items with live progress, pause/resume,
    /// edit destination, and delete controls.
    pub(super) fn view_queue_dialog(&self) -> Element<Message> {
        let t = self.theme;

        let summary = self.copy_queue.summary();

        // ── Controls row ─────────────────────────────────────────────────────
        let pause_label = if self.copy_queue.is_paused {
            "Resume"
        } else {
            "Pause"
        };
        let pause_btn = button(text(pause_label).size(11).color(t.accent))
            .style(t.transparent_button())
            .padding([3, 10])
            .on_press(Message::ToggleQueuePause);

        let status_text = summary.status_text();
        let controls = row![
            text("Copy Queue").size(13).color(t.text).width(Fill),
            text(status_text).size(11).color(t.text_secondary),
            pause_btn,
            button(text("X").size(11).color(t.text_secondary))
                .style(t.transparent_button())
                .padding([2, 6])
                .on_press(Message::CloseQueueDialog),
        ]
        .spacing(8)
        .padding([6, 10])
        .align_y(iced::Alignment::Center);

        // ── Item list ─────────────────────────────────────────────────────────
        let items: Vec<Element<Message>> = self
            .copy_queue
            .items
            .iter()
            .map(|item| {
                let is_editing = self
                    .queue_editing
                    .as_ref()
                    .map(|(id, _)| *id == item.id)
                    .unwrap_or(false);

                let name_text = text(item.display_name()).size(12).color(t.text);
                let id = item.id;

                // Status badge
                let status_color = match &item.status {
                    QueueStatus::Done => t.accent,
                    QueueStatus::Failed { .. } => t.error,
                    QueueStatus::Copying { .. } => t.text,
                    _ => t.text_secondary,
                };
                let status_badge = text(item.status.label()).size(10).color(status_color);

                // Progress bar (visible while copying)
                let progress_el: Element<Message> =
                    if let QueueStatus::Copying { percent } = &item.status {
                        progress_bar(0.0..=100.0, *percent as f32)
                            .height(4)
                            .into()
                    } else if let QueueStatus::Done = &item.status {
                        progress_bar(0.0..=100.0, 100.0).height(4).into()
                    } else {
                        iced::widget::Space::new(Fill, 4).into()
                    };

                // Destination display or edit input
                let dest_el: Element<Message> = if is_editing {
                    let current_input = self
                        .queue_editing
                        .as_ref()
                        .map(|(_, s)| s.as_str())
                        .unwrap_or("");
                    row![
                        text_input("Destination path", current_input)
                            .on_input(Message::QueueEditDestInput)
                            .on_submit(Message::QueueEditDestConfirm)
                            .size(11)
                            .padding([2, 6])
                            .width(Fill),
                        button(text("OK").size(10).color(t.accent))
                            .style(t.transparent_button())
                            .padding([2, 6])
                            .on_press(Message::QueueEditDestConfirm),
                    ]
                    .spacing(4)
                    .align_y(iced::Alignment::Center)
                    .into()
                } else {
                    text(item.android_dest.to_string_lossy().into_owned())
                        .size(10)
                        .color(t.text_secondary)
                        .into()
                };

                let action_btns = row![
                    button(text("Edit").size(10).color(t.text_secondary))
                        .style(t.transparent_button())
                        .padding([1, 6])
                        .on_press(Message::QueueEditItem(id)),
                    button(text("Remove").size(10).color(t.error))
                        .style(t.transparent_button())
                        .padding([1, 6])
                        .on_press(Message::QueueRemoveItem(id)),
                ]
                .spacing(2);

                container(
                    column![
                        row![name_text, iced::widget::Space::new(Fill, 1), status_badge, action_btns]
                            .spacing(4)
                            .align_y(iced::Alignment::Center),
                        dest_el,
                        progress_el,
                    ]
                    .spacing(3)
                    .padding([6, 10]),
                )
                .width(Fill)
                .style(move |_th| container::Style {
                    border: Border {
                        color: t.border,
                        width: 0.0,
                        ..Default::default()
                    },
                    background: Some(t.background_secondary.into()),
                    ..Default::default()
                })
                .into()
            })
            .collect();

        let list_body: Element<Message> = if items.is_empty() {
            container(
                text("Queue is empty. Select local files and click 'Copy to Device'.")
                    .size(11)
                    .color(t.text_secondary),
            )
            .padding([24, 16])
            .center_x(Fill)
            .into()
        } else {
            scrollable(
                column(items)
                    .spacing(2)
                    .padding([4, 0])
                    .width(Fill),
            )
            .height(iced::Length::Fixed(400.0))
            .into()
        };

        let card = container(
            column![
                container(controls).width(Fill).style(t.secondary_panel()),
                list_body,
            ]
            .width(Fill),
        )
        .width(580)
        .style(t.primary_panel());

        super::modal_backdrop(card)
    }
}
