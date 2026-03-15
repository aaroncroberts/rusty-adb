//! Modal overlay views: preview, log viewer, about, and settings.

use crate::adb::AdbStatus;
use crate::icons;
use crate::queue::QueueStatus;
use crate::{App, DeviceTab, LogLevel, Message, PreviewContent};
use iced::widget::{
    button, column, container, image, pick_list, progress_bar, rich_text, row, scrollable, text,
    text_input, toggler, Row,
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

        let close_btn = button(
                row![
                    text(icons::close()).font(icons::font()).size(12).color(t.text),
                    text("Close").size(12).color(t.text),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
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
    /// Shows a file picker (drop-down), a level filter, and color-coded log content.
    pub(super) fn view_log_viewer(&self) -> Element<Message> {
        let t = self.theme;

        // Retro terminal background — near-black with a faint green tint
        const TERM_BG: iced::Color = iced::Color::from_rgb(0.04, 0.06, 0.05);

        // Return the color for a log line based on its level keyword
        let line_color = |line: &str| -> iced::Color {
            let u = line.to_uppercase();
            if u.contains(" ERROR") || u.contains("[ERROR]") {
                iced::Color::from_rgb8(0xff, 0x55, 0x55) // bright red
            } else if u.contains(" WARN") || u.contains("[WARN]") {
                iced::Color::from_rgb8(0xff, 0xaa, 0x00) // amber
            } else if u.contains(" INFO") || u.contains("[INFO]") {
                t.accent // teal
            } else if u.contains(" DEBUG") || u.contains("[DEBUG]") {
                t.text_secondary
            } else if u.contains(" TRACE") || u.contains("[TRACE]") {
                t.text_secondary.scale_alpha(0.45)
            } else {
                t.text.scale_alpha(0.65) // continuation / unknown
            }
        };

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

        // ── Color-coded log content via rich_text ──────────────────────────
        // One span per line — much more efficient than one Text widget per line.
        // Annotate spans with Link=Message so the resulting element is Element<Message>.
        let log_content = if self.log_viewer_display.is_empty() {
            rich_text([
                iced::widget::text::Span::<'_, Message>::new("(no log content)")
                    .size(11)
                    .color(t.text_secondary)
                    .font(iced::Font::MONOSPACE),
            ])
        } else {
            let spans: Vec<iced::widget::text::Span<'_, Message>> = self
                .log_viewer_display
                .lines()
                .map(|line| {
                    iced::widget::text::Span::new(format!("{line}\n"))
                        .size(11)
                        .color(line_color(line))
                        .font(iced::Font::MONOSPACE)
                })
                .collect();
            rich_text(spans)
        };

        let log_scroll = scrollable(
            container(log_content)
                .padding([8, 12])
                .width(Fill),
        )
        .height(Fill);

        // ── Toolbar ────────────────────────────────────────────────────────
        let toolbar = row![
            text(icons::filter()).font(icons::font()).size(13).color(t.accent),
            text("File:").size(11).color(t.text_secondary),
            file_picker,
            iced::widget::horizontal_space(),
            text("Level:").size(11).color(t.text_secondary),
            level_picker,
            button(
                row![
                    text(icons::close()).font(icons::font()).size(11).color(t.text),
                    text("Close").size(11).color(t.text),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .style(t.secondary_button())
            .padding([2, 10])
            .on_press(Message::CloseLogViewer),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .padding([6, 10]);

        let card = container(
            column![
                // ── Header bar (retro terminal style) ────────────────────
                container(
                    row![
                        text(icons::eye())
                            .font(icons::font())
                            .size(14)
                            .color(t.accent),
                        text("  SYSTEM LOG")
                            .size(13)
                            .color(t.accent)
                            .font(iced::Font::MONOSPACE),
                        iced::widget::horizontal_space(),
                        text(
                            self.log_viewer_selected
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .unwrap_or("—"),
                        )
                        .size(11)
                        .color(t.text_secondary)
                        .font(iced::Font::MONOSPACE),
                    ]
                    .spacing(4)
                    .align_y(iced::Alignment::Center)
                    .padding([6, 10]),
                )
                .width(Fill)
                .style(move |_th| container::Style {
                    background: Some(t.accent.scale_alpha(0.08).into()),
                    border: Border {
                        color: t.accent.scale_alpha(0.3),
                        width: 0.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                // ── Toolbar row ──────────────────────────────────────────
                container(toolbar)
                    .width(Fill)
                    .style(move |_th| container::Style {
                        background: Some(t.background_secondary.scale_alpha(0.4).into()),
                        border: Border {
                            color: t.border,
                            width: 0.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                // ── Log content (retro terminal background) ───────────────
                container(log_scroll)
                    .width(Fill)
                    .height(Fill)
                    .style(move |_th| container::Style {
                        background: Some(TERM_BG.into()),
                        ..Default::default()
                    }),
            ]
            .width(Fill)
            .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(move |_th| container::Style {
            background: Some(TERM_BG.into()),
            border: Border {
                color: t.accent.scale_alpha(0.4),
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: iced::Shadow {
                color: iced::Color::BLACK.scale_alpha(0.6),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 32.0,
            },
            ..Default::default()
        });

        // Wrap in a padded container so the viewer has margins from the window edges.
        // This replaces the fixed max_width/max_height that caused overflow on small windows.
        let padded = container(card)
            .width(Fill)
            .height(Fill)
            .padding(36);

        // Use a custom backdrop that doesn't re-center (card is already Fill)
        let backdrop = container(padded)
            .width(Fill)
            .height(Fill)
            .style(|_th| container::Style {
                background: Some(iced::Color::from_rgba(0.02, 0.07, 0.06, 0.85).into()),
                ..Default::default()
            });

        iced::widget::mouse_area(backdrop)
            .on_press(Message::CloseLogViewer)
            .into()
    }

    /// About modal overlay — app info, version, GitHub link, license.
    pub(super) fn view_about_modal(&self) -> Element<Message> {
        let t = self.theme;
        let version = env!("CARGO_PKG_VERSION");

        let close_btn = button(
                row![
                    text(icons::close()).font(icons::font()).size(12).color(t.text),
                    text("Close").size(12).color(t.text),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
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

    /// Device Details modal — shows rich ADB property data for the connected device.
    pub(super) fn view_device_details_modal(&self) -> Element<Message> {
        let t = self.theme;
        let active_tab = self.device_details_tab;

        // ── Header ────────────────────────────────────────────────────────────
        let title = if let Some(ref d) = self.device_details {
            d.model.as_deref().unwrap_or("Connected Device").to_string()
        } else {
            "Device Details".to_string()
        };

        let close_btn = button(
                row![
                    text(icons::close()).font(icons::font()).size(12).color(t.text),
                    text("Close").size(12).color(t.text),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .style(t.secondary_button())
            .padding([4, 12])
            .on_press(Message::CloseDeviceDetails);

        let refresh_btn = button(
            row![
                text(icons::refresh()).font(icons::font()).size(12).color(t.accent),
                text("Refresh").size(12).color(t.accent),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center),
        )
            .style(t.transparent_button())
            .padding([4, 10])
            .on_press(Message::RefreshDeviceDetails);

        let header = container(
            row![
                text(title).size(14).color(t.text).width(Fill),
                refresh_btn,
                close_btn,
            ]
            .align_y(iced::Alignment::Center)
            .spacing(8)
            .padding([6, 10]),
        )
        .width(Fill)
        .style(t.secondary_panel());

        // ── Tab bar ───────────────────────────────────────────────────────────
        const TABS: [DeviceTab; 5] = [
            DeviceTab::Device,
            DeviceTab::OsBuild,
            DeviceTab::Connection,
            DeviceTab::Display,
            DeviceTab::Apps,
        ];

        let tab_bar = {
            let mut items: Vec<Element<Message>> = Vec::new();
            for tab in TABS {
                let is_active = tab == active_tab;
                let color = if is_active { t.accent } else { t.text_secondary };
                let bg_secondary = t.background_secondary;
                let border_color = t.border;
                let btn = button(text(tab.to_string()).size(12).color(color))
                    .style(move |_, _: iced::widget::button::Status| {
                        iced::widget::button::Style {
                            background: if is_active {
                                Some(bg_secondary.into())
                            } else {
                                None
                            },
                            border: if is_active {
                                iced::Border {
                                    color: border_color,
                                    width: 1.0,
                                    radius: 0.0.into(),
                                }
                            } else {
                                iced::Border::default()
                            },
                            ..Default::default()
                        }
                    })
                    .padding([4, 14])
                    .on_press(Message::DeviceDetailsSelectTab(tab));
                items.push(btn.into());
            }
            container(
                iced::widget::Row::from_vec(items)
                    .spacing(0)
                    .align_y(iced::Alignment::Center),
            )
            .width(Fill)
            .padding([0, 6])
            .style(t.secondary_panel())
        };

        // ── Shared helpers ────────────────────────────────────────────────────
        let na = "N/A";

        let kv_row = |key: &str, val: &str, monospace: bool| -> Row<Message> {
            let val_text = if monospace {
                text(val.to_string())
                    .size(11)
                    .color(t.text_secondary)
                    .font(iced::Font::MONOSPACE)
            } else {
                text(val.to_string()).size(12).color(t.text_secondary)
            };
            row![
                text(key.to_string()).size(12).color(t.text).width(160),
                val_text.width(Fill),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Start)
        };

        // ── Body ──────────────────────────────────────────────────────────────
        let body: Element<Message> = if self.device_details_loading && self.device_details.is_none() {
            container(
                text("Fetching device information…")
                    .size(12)
                    .color(t.text_secondary),
            )
            .width(Fill)
            .height(iced::Length::Fixed(240.0))
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .into()
        } else if let Some(ref d) = self.device_details {
            let is_emu = if d.is_emulator { "Yes" } else { "No" };

            let tab_content: Element<Message> = match active_tab {
                // ── Device ────────────────────────────────────────────────────
                DeviceTab::Device => {
                    let processor_str: String = match (
                        d.soc_manufacturer.as_deref(),
                        d.soc_model.as_deref(),
                    ) {
                        (Some(mfr), Some(mdl)) => format!("{mfr} {mdl}"),
                        (None, Some(mdl)) => mdl.to_string(),
                        (Some(mfr), None) => mfr.to_string(),
                        (None, None) => na.to_string(),
                    };
                    column![
                        kv_row("Model", d.model.as_deref().unwrap_or(na), false),
                        kv_row("Manufacturer", d.manufacturer.as_deref().unwrap_or(na), false),
                        kv_row("Brand", d.brand.as_deref().unwrap_or(na), false),
                        kv_row("Codename", d.device_codename.as_deref().unwrap_or(na), false),
                        kv_row("Serial", &d.serial, false),
                        kv_row("Emulator", is_emu, false),
                        kv_row("Processor", &processor_str, false),
                        kv_row("CPU ABI", d.cpu_abi.as_deref().unwrap_or(na), false),
                        kv_row("Total RAM", d.total_ram.as_deref().unwrap_or(na), false),
                    ]
                    .spacing(6)
                    .into()
                }

                // ── OS / Build ────────────────────────────────────────────────
                DeviceTab::OsBuild => column![
                    kv_row("Android Version", d.android_version.as_deref().unwrap_or(na), false),
                    kv_row("API Level", d.api_level.as_deref().unwrap_or(na), false),
                    kv_row("Security Patch", d.security_patch.as_deref().unwrap_or(na), false),
                    kv_row("Kernel Version", d.kernel_version.as_deref().unwrap_or(na), true),
                    kv_row("Uptime", d.uptime.as_deref().unwrap_or(na), false),
                    kv_row("Build Fingerprint", d.build_fingerprint.as_deref().unwrap_or(na), true),
                ]
                .spacing(6)
                .into(),

                // ── Connection ────────────────────────────────────────────────
                DeviceTab::Connection => column![
                    kv_row("Transport", d.transport.as_deref().unwrap_or(na), false),
                    kv_row("IP Address", d.ip_address.as_deref().unwrap_or(na), false),
                    kv_row("USB State", d.usb_state.as_deref().unwrap_or(na), false),
                    kv_row("Battery Level", d.battery_level.as_deref().unwrap_or(na), false),
                ]
                .spacing(6)
                .into(),

                // ── Display ───────────────────────────────────────────────────
                DeviceTab::Display => column![
                    kv_row("Resolution", d.screen_resolution.as_deref().unwrap_or(na), false),
                    kv_row("Density", d.screen_density.as_deref().unwrap_or(na), false),
                    kv_row("Characteristics", d.build_characteristics.as_deref().unwrap_or(na), false),
                    kv_row("Storage (/data)", d.storage_data.as_deref().unwrap_or(na), false),
                ]
                .spacing(6)
                .into(),

                // ── Apps ──────────────────────────────────────────────────────
                DeviceTab::Apps => {
                    if self.apps_loading {
                        container(text("Loading apps…").size(12).color(t.text_secondary))
                            .width(Fill)
                            .height(iced::Length::Fixed(200.0))
                            .align_x(iced::Alignment::Center)
                            .align_y(iced::Alignment::Center)
                            .into()
                    } else if let Some(ref apps) = self.installed_apps {
                        if apps.is_empty() {
                            container(
                                text("No user-installed apps found.")
                                    .size(12)
                                    .color(t.text_secondary),
                            )
                            .width(Fill)
                            .height(iced::Length::Fixed(200.0))
                            .align_x(iced::Alignment::Center)
                            .align_y(iced::Alignment::Center)
                            .into()
                        } else {
                            // Package list rows
                            let rows: Vec<Element<Message>> = apps
                                .iter()
                                .map(|app| {
                                    let is_sel =
                                        self.apps_selected.as_deref() == Some(&app.package_id);
                                    let bg = if is_sel {
                                        Some(t.background_secondary)
                                    } else {
                                        None
                                    };
                                    let pkg = app.package_id.clone();
                                    container(
                                        button(
                                            text(&app.package_id)
                                                .size(11)
                                                .color(if is_sel { t.accent } else { t.text }),
                                        )
                                        .style(t.transparent_button())
                                        .padding([3, 0])
                                        .width(Fill)
                                        .on_press(Message::AppsSelectPackage(pkg)),
                                    )
                                    .style(move |_| container::Style {
                                        background: bg.map(|c| c.into()),
                                        ..Default::default()
                                    })
                                    .width(Fill)
                                    .into()
                                })
                                .collect();

                            // Footer: confirm row when pending, otherwise uninstall button
                            let footer: Element<Message> = if self.uninstall_confirm {
                                let pkg_label = self
                                    .apps_selected
                                    .as_deref()
                                    .unwrap_or("this app");
                                let confirm_btn = button(
                                    text("Yes, Uninstall")
                                        .size(11)
                                        .color(iced::Color::WHITE),
                                )
                                .style(t.error_button())
                                .padding([3, 10])
                                .on_press(Message::UninstallConfirmed);
                                let cancel_btn = button(
                                    text("Cancel").size(11).color(t.text),
                                )
                                .style(t.transparent_button())
                                .padding([3, 8])
                                .on_press(Message::UninstallCancel);
                                container(
                                    row![
                                        text(format!(
                                            "Uninstall \"{}\"? This cannot be undone.",
                                            pkg_label
                                        ))
                                        .size(11)
                                        .color(t.text)
                                        .width(Fill),
                                        confirm_btn,
                                        cancel_btn,
                                    ]
                                    .spacing(6)
                                    .align_y(iced::Alignment::Center)
                                    .padding([4, 6]),
                                )
                                .width(Fill)
                                .style(t.warning_banner())
                                .into()
                            } else {
                                let uninstall_color =
                                    if self.apps_selected.is_some() && !self.apps_uninstalling {
                                        t.error
                                    } else {
                                        t.error.scale_alpha(0.3)
                                    };
                                let uninstall_label =
                                    if self.apps_uninstalling { "Uninstalling…" } else { "Uninstall" };
                                let b = button(
                                    text(uninstall_label).size(11).color(uninstall_color),
                                )
                                .style(t.transparent_button())
                                .padding([2, 8]);
                                let uninstall_btn: Element<Message> =
                                    if self.apps_selected.is_some() && !self.apps_uninstalling {
                                        b.on_press(Message::UninstallApp).into()
                                    } else {
                                        b.into()
                                    };
                                container(
                                    row![iced::widget::horizontal_space(), uninstall_btn]
                                        .align_y(iced::Alignment::Center),
                                )
                                .width(Fill)
                                .padding([6, 0])
                                .into()
                            };

                            column![
                                scrollable(column(rows).width(Fill).spacing(1))
                                    .height(iced::Length::Fixed(186.0)),
                                footer,
                            ]
                            .spacing(4)
                            .into()
                        }
                    } else {
                        // Not yet loaded — show a hint
                        container(
                            text("Select this tab to load the app list.")
                                .size(12)
                                .color(t.text_secondary),
                        )
                        .width(Fill)
                        .height(iced::Length::Fixed(200.0))
                        .align_x(iced::Alignment::Center)
                        .align_y(iced::Alignment::Center)
                        .into()
                    }
                }
            };

            scrollable(
                container(tab_content)
                    .width(Fill)
                    .padding([12, 16]),
            )
            .height(iced::Length::Fixed(280.0))
            .into()
        } else {
            container(
                text("No device data available.").size(12).color(t.text_secondary),
            )
            .width(Fill)
            .height(iced::Length::Fixed(80.0))
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .into()
        };

        let card = container(column![header, tab_bar, body].width(Fill))
            .width(600)
            .style(t.primary_panel());

        super::modal_backdrop(card)
    }

}
