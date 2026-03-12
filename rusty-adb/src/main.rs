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

mod adb;
mod status_bar;
mod theme;

use std::sync::Arc;
use std::time::Duration;

use adb::{AdbClient, AdbDevice, DeviceState};
use status_bar::{AdbStatus, StatusBar};
use theme::ThemeColors;

use iced::widget::{column, container, row, text, vertical_rule};
use iced::{Border, Element, Fill, Subscription, Task, Theme};

const TOOLBAR_HEIGHT: f32 = 40.0;

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    /// ADB binary located and daemon started — client is ready to use
    AdbReady(Arc<AdbClient>),
    /// Fired every 2 s by the Iced time subscription to trigger a device poll
    PollDevices,
    /// Result of `adb devices -l` — replaces the current device list
    DevicesLoaded(Arc<Vec<AdbDevice>>),
    /// ADB binary was not found or an error occurred during polling
    AdbError(String),
}

// ─── App State ────────────────────────────────────────────────────────────────

struct App {
    theme: ThemeColors,
    status_bar: StatusBar,
    adb_status: AdbStatus,
    /// Resolved ADB client; None if binary not found
    adb_client: Option<AdbClient>,
    /// Last known device list
    devices: Vec<AdbDevice>,
}

impl Default for App {
    fn default() -> Self {
        let theme = ThemeColors::dark();
        Self {
            status_bar: StatusBar::new(theme),
            adb_status: AdbStatus::Disconnected,
            adb_client: None,
            devices: Vec::new(),
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
        .subscription(App::subscription)
        .theme(|_| Theme::TokyoNightStorm)
        .window_size((1280.0, 800.0))
        .run_with(|| {
            // On startup: locate adb and warm the daemon in the background.
            let init_task = Task::perform(
                async {
                    let client = AdbClient::find().await.map_err(|e| e.to_string())?;
                    // Fire-and-forget: warm the ADB daemon
                    let _ = client.start_server().await;
                    Ok::<AdbClient, String>(client)
                },
                |result| match result {
                    Ok(client) => {
                        tracing::info!(adb = %client.adb_path.display(), "adb ready");
                        Message::AdbReady(Arc::new(client))
                    }
                    Err(e) => Message::AdbError(e),
                },
            );

            (App::default(), init_task)
        })
}

// ─── Subscription ─────────────────────────────────────────────────────────────

impl App {
    fn subscription(&self) -> Subscription<Message> {
        // Poll ADB every 2 seconds
        iced::time::every(Duration::from_secs(2)).map(|_| Message::PollDevices)
    }
}

// ─── Update ───────────────────────────────────────────────────────────────────

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AdbReady(client) => {
                self.adb_client = Some((*client).clone());
                // Immediately poll so the right pane updates without waiting 2s
                return self.update(Message::PollDevices);
            }

            Message::PollDevices => {
                let Some(client) = self.adb_client.clone() else {
                    // ADB not yet resolved — skip until AdbReady arrives
                    return Task::none();
                };

                Task::perform(
                    async move {
                        client
                            .list_devices()
                            .await
                            .map(|d| Arc::new(d))
                            .map_err(|e| e.to_string())
                    },
                    |result| match result {
                        Ok(devices) => Message::DevicesLoaded(devices),
                        Err(e) => Message::AdbError(e),
                    },
                )
            }

            Message::DevicesLoaded(devices) => {
                self.devices = (*devices).clone();
                self.adb_status = derive_status(&self.devices);
                tracing::debug!(count = self.devices.len(), "devices refreshed");
                Task::none()
            }

            Message::AdbError(msg) => {
                tracing::warn!(error = %msg, "adb error");
                self.adb_status = AdbStatus::Error(msg);
                Task::none()
            }
        }
    }
}

// ─── Status derivation ────────────────────────────────────────────────────────

/// Derive the status bar state from the current device list.
fn derive_status(devices: &[AdbDevice]) -> AdbStatus {
    // Pick the "best" device: authorized first, then unauthorized
    let authorized = devices.iter().find(|d| d.state == DeviceState::Device);
    let unauthorized = devices
        .iter()
        .find(|d| d.state == DeviceState::Unauthorized);

    if let Some(d) = authorized {
        AdbStatus::Connected(d.display_name().to_string())
    } else if unauthorized.is_some() {
        AdbStatus::Unauthorized
    } else {
        AdbStatus::Disconnected
    }
}

// ─── View ─────────────────────────────────────────────────────────────────────

impl App {
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

        // Header text reflects live connection state
        let header_text = match &self.adb_status {
            AdbStatus::Connected(name) => format!("Android Device — {}", name),
            AdbStatus::Unauthorized => {
                "Android Device — tap Allow on your phone".to_string()
            }
            AdbStatus::Connecting(name) => format!("Android Device — connecting to {}…", name),
            AdbStatus::Error(msg) => format!("Android Device — {}", msg),
            AdbStatus::Disconnected => "Android Device — No device connected".to_string(),
        };

        let header = container(
            text(header_text)
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
