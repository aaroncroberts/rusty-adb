//! rusty-adb — Android File Manager
//!
//! A fast, modern dual-pane file manager for transferring files between
//! a macOS host and Android devices via ADB.
//!
//! Layout:
//!   ┌─────────────────────────────────────────┐
//!   │              Toolbar (40px)              │
//!   ├──────────────────┬──────────────────────┤
//!   │  Local Files     │   Android Device     │
//!   │  (left pane)     │   (right pane)       │
//!   ├──────────────────┴──────────────────────┤
//!   │              Status Bar (30px)           │
//!   └─────────────────────────────────────────┘
#![allow(mismatched_lifetime_syntaxes)]

mod status_bar;
mod theme;

use status_bar::{AdbStatus, StatusBar};
use theme::ThemeColors;

use iced::widget::{column, container, row, text, vertical_rule};
use iced::{Border, Element, Fill, Task, Theme};

const TOOLBAR_HEIGHT: f32 = 40.0;

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    // Placeholder — filled out as features are implemented
}

// ─── App State ────────────────────────────────────────────────────────────────

struct App {
    theme: ThemeColors,
    status_bar: StatusBar,
    adb_status: AdbStatus,
}

impl Default for App {
    fn default() -> Self {
        let theme = ThemeColors::dark();
        Self {
            status_bar: StatusBar::new(theme),
            adb_status: AdbStatus::Disconnected,
            theme,
        }
    }
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

pub fn main() -> iced::Result {
    rusty_logging::LoggingConfig::builder()
        .with_console_compact()
        .with_file_text()
        .with_file_directory("./logs")
        .with_file_prefix("rusty-adb")
        .build()
        .expect("Invalid logging configuration")
        .apply()
        .expect("Failed to initialize logging");

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rusty-adb starting");

    iced::application("rusty-adb", App::update, App::view)
        .theme(|_| Theme::TokyoNightStorm)
        .window_size((1280.0, 800.0))
        .run()
}

// ─── Update ───────────────────────────────────────────────────────────────────

impl App {
    fn update(&mut self, _message: Message) -> Task<Message> {
        Task::none()
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    fn view(&self) -> Element<Message> {
        column![
            self.view_toolbar(),
            self.view_panes(),
            self.status_bar.view(&self.adb_status),
        ]
        .into()
    }

    /// Top toolbar: spans full width, holds connection/action buttons.
    fn view_toolbar(&self) -> Element<Message> {
        let t = self.theme;

        let label = text("rusty-adb").size(14).color(t.accent);

        let actions = text("  ·  Connect  ·  Copy →  ·  Copy ←  ·  Refresh")
            .size(12)
            .color(t.text_secondary);

        let content = row![label, actions]
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

    /// Middle dual pane: local filesystem (left) | Android device (right).
    fn view_panes(&self) -> Element<Message> {
        let left = self.view_left_pane();
        let right = self.view_right_pane();

        let divider = container(vertical_rule(1))
            .height(Fill)
            .style(move |_theme| container::Style {
                background: Some(self.theme.border.into()),
                ..Default::default()
            });

        row![left, divider, right]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Left pane: local macOS filesystem browser placeholder.
    fn view_left_pane(&self) -> Element<Message> {
        let t = self.theme;

        let header = container(text("Local Files").size(12).color(t.text_secondary))
            .width(Fill)
            .height(28.0)
            .padding([6, 12])
            .style(move |_theme| container::Style {
                background: Some(t.background_secondary.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            });

        let body = container(
            text("← Local file browser (task 3.1)")
                .size(12)
                .color(t.text_secondary),
        )
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(move |_theme| container::Style {
            background: Some(t.background.into()),
            ..Default::default()
        });

        column![header, body].width(Fill).height(Fill).into()
    }

    /// Right pane: Android device file browser placeholder.
    fn view_right_pane(&self) -> Element<Message> {
        let t = self.theme;

        let header = container(
            text("Android Device — No device connected")
                .size(12)
                .color(t.text_secondary),
        )
        .width(Fill)
        .height(28.0)
        .padding([6, 12])
        .style(move |_theme| container::Style {
            background: Some(t.background_secondary.into()),
            border: Border {
                color: t.border,
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        });

        let body = container(
            text("→ Android file browser (task 4.2)")
                .size(12)
                .color(t.text_secondary),
        )
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(move |_theme| container::Style {
            background: Some(t.background.into()),
            ..Default::default()
        });

        column![header, body].width(Fill).height(Fill).into()
    }
}
