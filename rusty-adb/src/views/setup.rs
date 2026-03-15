//! ADB-not-found setup guide screen.

use crate::{App, Message};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Fill};

impl App {
    pub(super) fn view_adb_not_found(&self) -> Element<Message> {
        use iced::widget::Space;
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
            .style(t.accent_button())
            .padding([8, 16])
            .on_press(Message::InstallAdb);

        let retry_btn = button(text("Retry Detection").size(13).color(t.text))
            .style(t.secondary_button())
            .padding([8, 16])
            .on_press(Message::RetryAdbFind);

        let download_btn = button(text("Download Manually").size(13).color(t.accent))
            .style(t.transparent_button())
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
                .style(t.secondary_panel()),
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
                .size(12)
                .color(t.text_secondary),
            Space::with_height(6),
            text("  1.  Settings › About Phone › tap Build Number 7 times")
                .size(12)
                .color(t.text_secondary),
            text("      Samsung:  About Phone › Software Information › Build Number")
                .size(11)
                .color(t.text_secondary),
            text("      Xiaomi:   About Phone › tap MIUI Version 7 times")
                .size(11)
                .color(t.text_secondary),
            text("      OnePlus:  About Device › Version › Build Number")
                .size(11)
                .color(t.text_secondary),
            Space::with_height(4),
            text("  2.  Settings › Developer Options › enable USB Debugging")
                .size(12)
                .color(t.text_secondary),
            Space::with_height(4),
            text("  3.  Plug in USB cable and set USB mode to \"File Transfer\"")
                .size(12)
                .color(t.text_secondary),
            text("      Swipe down the notification shade and tap the USB notification.")
                .size(11)
                .color(t.text_secondary),
            Space::with_height(4),
            text("  4.  Tap \"Allow\" on the USB Debugging dialog on your phone")
                .size(12)
                .color(t.text_secondary),
            text("      Tick \"Always allow from this computer\" to skip this next time.")
                .size(11)
                .color(t.text_secondary),
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
        let card =
            container(
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
            .style(t.secondary_panel());

        container(card)
            .width(Fill)
            .height(Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}
