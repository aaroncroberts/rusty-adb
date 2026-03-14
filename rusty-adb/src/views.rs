//! View implementations for `App`.
use super::{App, Message, ViewMode, LogLevel, PreviewContent, TOOLBAR_HEIGHT};
use std::path::PathBuf;
use crate::adb::AdbClient;
use crate::android_fs::AndroidContext;
use crate::file_pane::{view_breadcrumb, RenameCbs};
use crate::status_bar::AdbStatus;
use crate::adb::DeviceState;
use iced::widget::{
    button, column, container, image, pick_list, row, scrollable, stack, text,
    text_input, toggler, vertical_rule, Row,
};
use iced::{Border, Element, Fill, Theme};

impl App {
    pub(super) fn view(&self) -> Element<Message> {
        // ADB not installed — replace panes with the setup guide
        if self.adb_status == AdbStatus::NotFound {
            let mut items: Vec<Element<Message>> = vec![self.view_toolbar()];
            items.push(self.view_adb_not_found());
            items.push(self.status_bar.view(&self.adb_status, None, None));
            return column(items).into();
        }

        let mut items: Vec<Element<Message>> = vec![self.view_hero_banner(), self.view_toolbar()];
        if let Some(msg) = &self.error_banner {
            items.push(self.view_error_banner(msg));
        }
        if let Some(msg) = &self.toast {
            items.push(self.view_toast_banner(msg));
        }
        if let Some(paths) = &self.delete_confirm_paths {
            items.push(self.view_delete_confirm(paths));
        }
        items.push(self.view_panes());
        items.push(
            self.status_bar.view(
                &self.adb_status,
                self.transfer_status.as_ref(),
                self.active_transfer
                    .as_ref()
                    .map(|_| Message::CancelTransfer),
            ),
        );

        let base: Element<Message> = column(items).into();

        // Stack-based modal overlays (log viewer > settings > about > preview)
        if self.log_viewer_open {
            stack![base, self.view_log_viewer()].into()
        } else if self.settings_open {
            stack![base, self.view_settings_modal()].into()
        } else if self.about_open {
            stack![base, self.view_about_modal()].into()
        } else if let Some(modal_content) = &self.preview_modal {
            stack![base, self.view_preview_modal(modal_content)].into()
        } else {
            base
        }
    }

    fn view_adb_not_found(&self) -> Element<Message> {
        use iced::widget::{scrollable, Space};
        use iced::Alignment;

        let t = self.theme;

        // ── Install button (platform-specific) ───────────────────────────────
        #[cfg(target_os = "macos")]
        let install_label = "Install via Homebrew";
        #[cfg(target_os = "windows")]
        let install_label = "Install via winget";
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let install_label = "Download Platform Tools";

        let install_btn = button(text(install_label).size(13).color(iced::Color::WHITE))
            .style(move |_t, _s| button::Style {
                background: Some(t.accent.into()),
                border: Border {
                    radius: 0.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .padding([8, 16])
            .on_press(Message::InstallAdb);

        let retry_btn = button(text("Retry Detection").size(13).color(t.text))
            .style(move |_t, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .padding([8, 16])
            .on_press(Message::RetryAdbFind);

        let download_btn = button(text("Download Manually").size(13).color(t.accent))
            .style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .on_press(Message::OpenUrl(
                "https://developer.android.com/tools/releases/platform-tools".to_string(),
            ));

        // ── Install log output (shown during/after install attempt) ──────────
        let log_section: Element<Message> = if self.install_log.is_empty() {
            Space::with_height(0).into()
        } else {
            let log_text = self.install_log.join("\n");
            scrollable(
                container(
                    text(log_text)
                        .size(11)
                        .color(t.text_secondary)
                        .font(iced::Font::with_name("Menlo")),
                )
                .width(Fill)
                .padding([8, 12])
                .style(move |_t| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                }),
            )
            .height(120)
            .into()
        };

        // ── Platform-specific install steps ──────────────────────────────────
        #[cfg(target_os = "macos")]
        let steps: Element<Message> = column![
            text("Option A — Homebrew (recommended)")
                .size(12)
                .color(t.text_secondary),
            text("  brew install --cask android-platform-tools")
                .size(12)
                .color(t.accent)
                .font(iced::Font::with_name("Menlo")),
            Space::with_height(8),
            text("Option B — Android Studio")
                .size(12)
                .color(t.text_secondary),
            text("  Open SDK Manager → SDK Tools → Android SDK Platform-Tools")
                .size(12)
                .color(t.text_secondary),
            text("  SDK installs to ~/Library/Android/sdk/platform-tools/")
                .size(11)
                .color(t.text_secondary),
        ]
        .spacing(4)
        .into();

        #[cfg(target_os = "windows")]
        let steps: Element<Message> = column![
            text("Option A — winget (recommended)")
                .size(12)
                .color(t.text_secondary),
            text("  winget install Google.PlatformTools")
                .size(12)
                .color(t.accent)
                .font(iced::Font::with_name("Menlo")),
            Space::with_height(8),
            text("Option B — Android Studio")
                .size(12)
                .color(t.text_secondary),
            text("  Open SDK Manager → SDK Tools → Android SDK Platform-Tools")
                .size(12)
                .color(t.text_secondary),
            text("  SDK installs to %LOCALAPPDATA%\\Android\\Sdk\\platform-tools\\")
                .size(11)
                .color(t.text_secondary),
        ]
        .spacing(4)
        .into();

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let steps: Element<Message> = column![
            text("Download from developer.android.com/tools/releases/platform-tools")
                .size(12)
                .color(t.text_secondary),
            text("  Extract and ensure 'adb' is on your PATH")
                .size(12)
                .color(t.text_secondary),
        ]
        .spacing(4)
        .into();

        // ── Developer mode steps (same on all platforms) ─────────────────────
        let dev_steps: Element<Message> = column![
            text("After installing ADB, enable Developer Mode on your phone:")
                .size(12).color(t.text_secondary),
            Space::with_height(6),
            text("  1.  Settings › About Phone › tap Build Number 7 times")
                .size(12).color(t.text_secondary),
            text("      Samsung:  About Phone › Software Information › Build Number")
                .size(11).color(t.text_secondary),
            text("      Xiaomi:   About Phone › tap MIUI Version 7 times")
                .size(11).color(t.text_secondary),
            text("      OnePlus:  About Device › Version › Build Number")
                .size(11).color(t.text_secondary),
            Space::with_height(4),
            text("  2.  Settings › Developer Options › enable USB Debugging")
                .size(12).color(t.text_secondary),
            Space::with_height(4),
            text("  3.  Plug in USB cable and set USB mode to \"File Transfer\"")
                .size(12).color(t.text_secondary),
            text("      Swipe down the notification shade and tap the USB notification.")
                .size(11).color(t.text_secondary),
            Space::with_height(4),
            text("  4.  Tap \"Allow\" on the USB Debugging dialog on your phone")
                .size(12).color(t.text_secondary),
            text("      Tick \"Always allow from this computer\" to skip this next time.")
                .size(11).color(t.text_secondary),
        ]
        .spacing(2)
        .into();

        let divider = container(Space::with_height(1))
            .width(Fill)
            .style(move |_t| container::Style {
                background: Some(t.border.into()),
                ..Default::default()
            });

        // ── Main card layout ─────────────────────────────────────────────────
        let card = container(
            scrollable(
                column![
                    text("Setup Guide").size(22).color(t.text),
                    Space::with_height(4),
                    text("ADB gives rusty-adb full read/write access to your device's filesystem.")
                        .size(13).color(t.text_secondary),
                    Space::with_height(20),
                    text("Step 1 — Install ADB").size(14).color(t.text),
                    Space::with_height(8),
                    steps,
                    Space::with_height(12),
                    row![install_btn, retry_btn, download_btn].spacing(12),
                    Space::with_height(8),
                    log_section,
                    Space::with_height(20),
                    divider,
                    Space::with_height(20),
                    text("Step 2 — Enable Developer Mode on your phone")
                        .size(14).color(t.text),
                    Space::with_height(8),
                    dev_steps,
                ]
                .spacing(0)
                .width(560)
                .padding([0, 4]),
            )
            .height(Fill),
        )
        .padding(32)
        .style(move |_t| container::Style {
            background: Some(t.background_secondary.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        container(card)
            .width(Fill)
            .height(Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    fn view_error_banner<'a>(&'a self, msg: &'a str) -> Element<'a, Message> {
        let t = self.theme;
        let content = row![
            text(format!("[!] {msg}")).size(12).color(t.error),
            iced::widget::Space::with_width(Fill),
            button(text("X").size(11).color(t.error))
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(Message::DismissError),
        ]
        .align_y(iced::Alignment::Center)
        .padding([4, 12]);

        container(content)
            .width(Fill)
            .style(move |_t| container::Style {
                background: Some(t.error.scale_alpha(0.12).into()),
                border: Border {
                    color: t.error.scale_alpha(0.4),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// Delete confirmation banner — shown above the panes when paths are pending deletion.
    fn view_delete_confirm<'a>(&'a self, paths: &'a [PathBuf]) -> Element<'a, Message> {
        let t = self.theme;
        let label = if paths.len() == 1 {
            format!(
                "Delete \"{}\"? This cannot be undone.",
                paths[0].file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!("Delete {} items? This cannot be undone.", paths.len())
        };

        let confirm_btn = button(text("Confirm Delete").size(12).color(iced::Color::WHITE))
            .style(move |_t, _s| button::Style {
                background: Some(t.error.into()),
                border: Border {
                    radius: 0.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .padding([4, 12])
            .on_press(Message::AndroidDeleteConfirm);

        let cancel_btn = button(text("Cancel").size(12).color(t.text))
            .style(move |_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .padding([4, 8])
            .on_press(Message::AndroidDeleteCancel);

        let content = row![
            text(label).size(12).color(t.text).width(Fill),
            confirm_btn,
            cancel_btn,
        ]
        .align_y(iced::Alignment::Center)
        .spacing(8)
        .padding([4, 12]);

        container(content)
            .width(Fill)
            .style(move |_t| container::Style {
                background: Some(t.warning.scale_alpha(0.15).into()),
                border: Border {
                    color: t.warning.scale_alpha(0.5),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// Preview modal overlay — rendered on top of the full UI via `stack!`.
    ///
    /// Wraps `content` in a full-window semi-transparent backdrop.
    ///
    /// Shared by all three modal overlays (preview, about, settings).
    fn modal_backdrop<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        container(
            container(content)
                .center_x(Fill)
                .center_y(Fill)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(|_th| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6).into()),
            ..Default::default()
        })
        .into()
    }

    /// The semi-transparent backdrop captures clicks (closing the modal).
    /// The inner card shows either an image or scrollable text.
    fn view_preview_modal<'a>(&'a self, content: &'a PreviewContent) -> Element<'a, Message> {
        let t = self.theme;

        let close_btn = button(text("X  Close").size(12).color(t.text))
            .style(move |_th, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
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
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
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
        .style(move |_th| container::Style {
            background: Some(t.background.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        // Semi-transparent backdrop — fills the full window
        Self::modal_backdrop(card)
    }

    /// Log viewer modal overlay.
    ///
    /// Shows a file picker (drop-down), a level filter, and the log content.
    fn view_log_viewer(&self) -> Element<Message> {
        let t = self.theme;

        // ── File picker ────────────────────────────────────────────────────
        // We pass paths directly to pick_list — PathBuf implements Display via its Debug.
        // To show just the filename, wrap in a newtype or use the file_name component.
        // Simplest approach: build a Vec<String> of basenames and find the path by name on select.
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

        // ── Filter log lines by level ──────────────────────────────────────
        let level = self.log_viewer_level;
        let filtered_lines: String = self
            .log_viewer_content
            .lines()
            .filter(|l| level.matches(l))
            .collect::<Vec<_>>()
            .join("\n");

        let log_display = if filtered_lines.is_empty() {
            "(No lines match the selected level filter)".to_string()
        } else {
            filtered_lines
        };

        let log_scroll = scrollable(
            container(
                text(log_display).size(11).font(iced::Font::MONOSPACE).color(t.text),
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
                .style(move |_th: &Theme, _s| button::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                })
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
                    row![
                        text("Log Viewer").size(14).color(t.text).width(Fill),
                    ]
                    .padding([6, 10]),
                )
                .width(Fill)
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
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

        Self::modal_backdrop(card)
    }

    /// About modal overlay — app info, version, GitHub link, license.
    fn view_about_modal(&self) -> Element<Message> {
        let t = self.theme;
        let version = env!("CARGO_PKG_VERSION");

        let close_btn = button(text("X  Close").size(12).color(t.text))
            .style(move |_th, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .padding([4, 12])
            .on_press(Message::CloseAbout);

        let github_btn = button(
            text("github.com/aaroncroberts/rusty-adb")
                .size(12)
                .color(t.accent),
        )
        .style(move |_th, _s| button::Style {
            background: None,
            ..Default::default()
        })
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
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                // Body
                container(
                    column![
                        // ASCII art logo — "rusty" (left) + "adb" (right), figlet small
                        column![
                            text("╔══════════════════════════════════════════════╗").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                            text("║ _ __ _   _ ___| |_ _   _    __ _  __| | |__ ║").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                            text("║| '__| | | / __| __| | | |  / _` |/ _` | '_ \\║").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                            text("║| |  | |_| \\__ \\ |_| |_| | | (_| | (_| | |_) |║").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                            text("║|_|   \\__,_|___/\\__|\\__, | \\__,_|\\__,_|_.__/ ║").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                            text("║                      |___/                    ║").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                            text(format!("║  Android file manager  v{version:<21}║")).size(12).font(iced::Font::MONOSPACE).color(t.text),
                            text("╚══════════════════════════════════════════════╝").size(12).font(iced::Font::MONOSPACE).color(t.accent),
                        ]
                        .spacing(0),
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
        .style(move |_th| container::Style {
            background: Some(t.background.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        Self::modal_backdrop(card)
    }

    /// Success/info toast banner (green tint, auto-dismisses after 3 s).
    fn view_toast_banner<'a>(&'a self, msg: &'a str) -> Element<'a, Message> {
        let t = self.theme;
        let content = row![text(msg).size(12).color(t.text).width(Fill),].padding([6, 15]);
        container(content)
            .width(Fill)
            .style(move |_th| container::Style {
                background: Some(t.success.scale_alpha(0.15).into()),
                border: Border {
                    color: t.success.scale_alpha(0.5),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// Settings modal overlay — rendered on top of the full UI via `stack!`.
    fn view_settings_modal(&self) -> Element<Message> {
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
            .style(move |_th, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .on_press(Message::OpenLogFolder);

        let view_logs_btn = button(text("View Logs").size(12).color(t.accent))
            .style(move |_th: &Theme, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .on_press(Message::OpenLogViewer);

        let save_btn = button(text("Save").size(12).color(iced::Color::WHITE))
            .style(move |_th, _s| button::Style {
                background: Some(t.accent.into()),
                border: Border {
                    radius: 0.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .padding([5, 16])
            .on_press(Message::SaveSettings);

        let cancel_btn = button(text("Cancel").size(12).color(t.text))
            .style(move |_th, _s| button::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
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
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
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
                                        .style(move |_th, _s| button::Style {
                                            background: Some(t.background_secondary.into()),
                                            border: Border {
                                                color: t.border,
                                                width: 1.0,
                                                radius: 0.0.into(),
                                            },
                                            ..Default::default()
                                        })
                                        .padding([5, 14]);
                                    let elem: Element<Message> = if let Some(m) = msg {
                                        b.on_press(m).into()
                                    } else {
                                        b.into()
                                    };
                                    elem
                                };
                            let refresh = action_btn("Refresh", t.text, Some(Message::RefreshPanes));
                            let disconnect = action_btn(
                                "Disconnect",
                                if has_device { t.warning } else { t.text_secondary },
                                if has_device { Some(Message::DisconnectDevice) } else { None },
                            );
                            let restart = action_btn(
                                "Restart Daemon",
                                if daemon_error { t.warning } else { t.text },
                                Some(Message::RestartDaemon),
                            );
                            let r: Element<Message> = Row::from_vec(vec![refresh, disconnect, restart])
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
                .style(move |_th| container::Style {
                    background: Some(t.background_secondary.into()),
                    border: Border {
                        color: t.border,
                        width: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            ]
            .width(Fill),
        )
        .width(420)
        .style(move |_th| container::Style {
            background: Some(t.background.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        Self::modal_backdrop(card)
    }

    fn view_toolbar(&self) -> Element<Message> {
        let t = self.theme;

        // ── Button builder helper ─────────────────────────────────────────────
        let toolbar_btn = move |label: String, color: iced::Color, msg: Option<Message>| {
            let lbl = text(label).size(12).color(color);
            let btn = button(lbl)
                .padding([4, 10])
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });
            if let Some(m) = msg {
                btn.on_press(m)
            } else {
                btn
            }
        };

        // ── Buttons (toolbar — device actions moved to Settings modal) ────────
        let settings_btn = toolbar_btn("Settings".to_string(), t.text, Some(Message::OpenSettings));
        let about_btn = toolbar_btn("About".to_string(), t.text_secondary, Some(Message::OpenAbout));

        let content = row![settings_btn, about_btn,]
            .spacing(8)
            .padding([0, 16])
            .align_y(iced::Alignment::Center);

        container(content)
            .width(Fill)
            .height(TOOLBAR_HEIGHT)
            .style(move |_theme| container::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    // ── Hero Banner ────────────────────────────────────────────────────────────

    /// Retro ASCII hero banner — a slim persistent strip above the pane menu bars.
    ///
    /// Shows: ASCII logotype · version · tagline, in phosphor-green palette.
    fn view_hero_banner<'a>(&self) -> Element<'a, Message> {
        let t = self.theme;
        let version = env!("CARGO_PKG_VERSION");

        // Top row: ASCII logotype + version
        let logo_line = format!(
            "◄◄ RUSTY-ADB ►► v{}",
            version
        );
        let logo = text(logo_line)
            .size(13)
            .color(t.accent)
            .font(iced::Font::MONOSPACE);

        // Bottom row: tagline
        let tagline = text("Android file manager · Rust + Iced")
            .size(9)
            .color(t.text_secondary)
            .font(iced::Font::MONOSPACE);

        let inner = column![logo, tagline]
            .spacing(1)
            .padding([4, 12])
            .align_x(iced::Alignment::End);

        container(row![iced::widget::Space::with_width(Fill), inner])
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.accent.scale_alpha(0.35),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    // ── Pane Menu Bar ──────────────────────────────────────────────────────────

    /// Horizontal menu bar rendered at the top of each directory pane.
    ///
    /// Contains (left to right):
    /// - View mode toggle group: [List] [Details] [Grid] [Icon]
    /// - Spacer
    /// - File command buttons (pane-specific, enabled/disabled by selection)
    /// - Hidden files toggle: [.Hidden] / [.Shown]
    fn view_pane_menu_bar<'a>(
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
                    .style(move |_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    });
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
            .style(move |_theme| container::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    // ── Grid / Icon view renderers ─────────────────────────────────────────────

    /// Render local entries as 4-column compact tiles (Grid view).
    fn view_local_grid(&self) -> Element<Message> {
        self.view_grid_impl(false)
    }

    /// Render android entries as 4-column compact tiles (Grid view).
    fn view_android_grid(&self) -> Element<Message> {
        self.view_grid_impl(true)
    }

    /// Render local entries as 2-column large tiles (Icon view).
    fn view_local_icon(&self) -> Element<Message> {
        self.view_icon_impl(false)
    }

    /// Render android entries as 2-column large tiles (Icon view).
    fn view_android_icon(&self) -> Element<Message> {
        self.view_icon_impl(true)
    }

    /// Finder "As Gallery": large preview at top, horizontal filmstrip of all entries at bottom.
    fn view_grid_impl(&self, is_android: bool) -> Element<Message> {
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
                text("This directory is empty").size(11).color(t.text_secondary),
            )
            .padding([20, 20])
            .width(Fill)
            .height(Fill)
            .into();
        }

        // ── Large preview pane ────────────────────────────────────────────────
        let (_, preview_name, preview_is_dir, _) = visible[gallery_idx].clone();
        let preview_fg = if preview_is_dir { t.accent } else { t.text_secondary };

        let ext = preview_name.rsplit('.').next()
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        let is_image = !preview_is_dir && !is_android
            && matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp");

        let large_preview: Element<Message> = if is_image {
            image(image::Handle::from_path(current_path.join(preview_name.as_str())))
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
            let ext2 = name.rsplit('.').next()
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            let is_img = !is_dir && !is_android
                && matches!(ext2.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp");

            let thumb: Element<Message> = if is_img {
                image(image::Handle::from_path(current_path.join(name.as_str())))
                    .width(STRIP_TILE_W - 8)
                    .height(STRIP_TILE_H - 22)
                    .into()
            } else {
                let art = if is_dir { "┌──┐\n│/ │\n└──┘" } else { "┌──┐\n│──│\n└──┘" };
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
                        text(short).size(9).font(iced::Font::MONOSPACE).color(name_fg),
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
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
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
            .style(move |_th| container::Style {
                background: Some(t.background.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

        column![preview_area, filmstrip_container]
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn view_icon_impl(&self, is_android: bool) -> Element<Message> {
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
                    (i, e.name.clone(), e.is_navigable(), self.android_pane.selected.contains(&i), e.is_hidden)
                })
                .collect()
        } else {
            self.local_pane
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| self.local_pane.show_hidden || !e.is_hidden)
                .map(|(i, e)| (i, e.name.clone(), e.is_navigable(), self.local_pane.selected.contains(&i), e.is_hidden))
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
                container(text("This directory is empty").size(11).color(t.text_secondary))
                    .padding([8, 12]),
            );
        } else {
            for chunk in visible.chunks(COLS) {
                let mut tile_row: Vec<Element<Message>> = Vec::new();
                for &(orig_idx, ref name, can_navigate, is_selected, is_hidden) in chunk {
                    let bg: Option<iced::Background> =
                        if is_selected { Some(t.accent.scale_alpha(0.20).into()) } else { None };
                    let border_color = if is_selected { t.accent } else { t.border };
                    let border_width = if is_selected { 2.0 } else { 1.0 };
                    let icon_fg = if can_navigate { t.accent } else { t.text_secondary };
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

                    let ext = name.rsplit('.').next()
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let is_image = !can_navigate && !is_android
                        && matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp");

                    // 3-line icon art — compact enough to leave room for the filename
                    let icon_art: Element<Message> = if is_image {
                        image(image::Handle::from_path(current_path.join(name.as_str())))
                            .width(56)
                            .height(44)
                            .into()
                    } else {
                        let art = if can_navigate { "┌───┐\n│ / │\n└───┘" } else { "┌───┐\n│───│\n└───┘" };
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
                    iced::widget::Row::from_vec(tile_row).spacing(10).padding([0, 2]),
                );
            }
        }

        scrollable(col.width(Fill)).height(Fill).into()
    }

    fn view_panes(&self) -> Element<Message> {
        let t = self.theme;
        let has_device = self.active_serial.is_some();
        let no_transfer = self.active_transfer.is_none();

        // ── Precompute selection flags ────────────────────────────────────────
        let local_sel_has_files = self.local_pane.selected.iter().any(|&i| {
            self.local_pane.entries.get(i).map(|e| !e.is_dir).unwrap_or(false)
        });
        let android_sel_has_files = self.android_pane.selected.iter().any(|&i| {
            self.android_pane.entries.get(i).map(|e| !e.is_dir).unwrap_or(false)
        });
        let android_sel_single =
            self.android_pane.selected.len() == 1 && self.android_pane.rename_pending.is_none();
        let android_sel_nonempty =
            !self.android_pane.selected.is_empty() && self.android_pane.rename_pending.is_none();

        // ── Local pane ────────────────────────────────────────────────────────
        let local_menu = self.view_pane_menu_bar(
            false,
            self.local_view_mode,
            self.local_pane.show_hidden,
            local_sel_has_files,
            android_sel_has_files,
            android_sel_single,
            android_sel_nonempty,
            has_device,
            no_transfer,
        );

        let local_content: Element<Message> = match self.local_view_mode {
            // Name-only rows with type icons — compact, maximum density
            ViewMode::List => {
                let breadcrumb = view_breadcrumb(
                    &self.local_pane.current_path, self.theme, Message::LocalNavigateTo,
                );
                let list = self.local_pane.view_list(
                    &(),
                    self.theme,
                    Message::LocalNavigateTo,
                    Message::LocalSelectEntry,
                    Message::LocalSortBy,
                    None,
                );
                column![breadcrumb, list].width(Fill).height(Fill).into()
            }
            // Finder-style column view: path ancestors on left, entries on right
            ViewMode::Details => self.view_columns_impl(false),
            ViewMode::Grid => self.view_local_grid(),
            ViewMode::Icon => self.view_local_icon(),
        };

        let local_path = self.local_pane.current_path.display().to_string();
        let local_title = self.view_pane_title_bar("LOCAL", &local_path);

        let left: Element<Message> = column![local_title, local_menu, local_content]
            .width(Fill)
            .height(Fill)
            .into();

        // ── Android pane ──────────────────────────────────────────────────────
        let android_menu = self.view_pane_menu_bar(
            true,
            self.android_view_mode,
            self.android_pane.show_hidden,
            local_sel_has_files,
            android_sel_has_files,
            android_sel_single,
            android_sel_nonempty,
            has_device,
            no_transfer,
        );

        let default_android_ctx = AndroidContext {
            client: AdbClient { adb_path: PathBuf::new() },
            serial: String::new(),
            storage_roots: Vec::new(),
            device_label: String::new(),
        };
        let android_ctx_ref = self.android_ctx.as_ref().unwrap_or(&default_android_ctx);

        let android_content: Element<Message> = match self.android_view_mode {
            // Name-only rows with type icons
            ViewMode::List => {
                let breadcrumb = view_breadcrumb(
                    &self.android_pane.current_path, self.theme, Message::AndroidNavigateTo,
                );
                let list = self.android_pane.view_list(
                    android_ctx_ref,
                    self.theme,
                    Message::AndroidNavigateTo,
                    Message::AndroidSelectEntry,
                    Message::AndroidSortBy,
                    Some(RenameCbs {
                        on_input: Box::new(Message::AndroidRenameInput),
                        on_commit: Message::AndroidRenameCommit,
                    }),
                );
                column![breadcrumb, list].width(Fill).height(Fill).into()
            }
            // Finder-style column view: path ancestors on left, entries on right
            ViewMode::Details => self.view_columns_impl(true),
            ViewMode::Grid => self.view_android_grid(),
            ViewMode::Icon => self.view_android_icon(),
        };

        // Wrap android content with drop-zone highlight
        let hover = self.file_hover_active;
        let android_content: Element<Message> = container(android_content)
            .width(Fill)
            .height(Fill)
            .style(move |_theme| container::Style {
                border: Border {
                    color: if hover {
                        t.accent.scale_alpha(0.8)
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if hover { 2.0 } else { 0.0 },
                    ..Default::default()
                },
                ..Default::default()
            })
            .into();

        let android_path = self.android_pane.current_path.display().to_string();
        let android_title = self.view_pane_title_bar("ANDROID", &android_path);

        let right: Element<Message> = column![android_title, android_menu, android_content]
            .width(Fill)
            .height(Fill)
            .into();

        let divider = container(vertical_rule(1))
            .height(Fill)
            .style(move |_theme| container::Style {
                background: Some(t.border.into()),
                ..Default::default()
            });

        row![left, divider, right].width(Fill).height(Fill).into()
    }

    /// Finder "As Columns": path ancestors on the left, current entries on the right.
    ///
    /// Left column shows all ancestor directories of the current path as clickable
    /// entries. Right column renders the current directory as a list view.
    fn view_columns_impl(&self, is_android: bool) -> Element<Message> {
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

        let ancestor_panel = container(
            scrollable(ancestor_col.width(Fill)).height(Fill),
        )
        .width(180)
        .height(Fill)
        .style(move |_th| container::Style {
            background: Some(t.background_secondary.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        // ── Right: current directory entry list ───────────────────────────────
        let default_ctx = AndroidContext {
            client: AdbClient { adb_path: PathBuf::new() },
            serial: String::new(),
            storage_roots: Vec::new(),
            device_label: String::new(),
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

    /// Box-drawing title bar rendered above each file-browser pane.
    ///
    /// Produces a single line like:  `┌─ LOCAL ─ /home/user ──`
    /// in monospace accent color on a secondary-background strip.
    fn view_pane_title_bar<'a>(&self, label: &str, path: &str) -> Element<'a, Message> {
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
        .style(move |_th| container::Style {
            background: Some(t.background_secondary.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }
}
