//! Modal overlay views: preview, log viewer, about, and settings.

use crate::adb::AdbStatus;
use crate::{App, LogLevel, Message, PreviewContent};
use iced::widget::{
    button, column, container, image, pick_list, row, scrollable, text, toggler, Row,
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
}
