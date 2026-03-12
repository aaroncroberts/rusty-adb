//! Status bar component for rusty-adb
//!
//! Renders a horizontal bar at the bottom of the window showing ADB
//! connection state and (later) file transfer progress.

use crate::theme::ThemeColors;
use iced::widget::{container, row, text};
use iced::{Border, Element, Fill};

/// Height of the status bar in pixels
pub const STATUS_BAR_HEIGHT: f32 = 30.0;

/// ADB device connection status
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // variants used when Android pane is implemented (task 4.1)
pub enum AdbStatus {
    /// No device connected
    Disconnected,
    /// Device found but user hasn't authorized USB debugging yet
    Unauthorized,
    /// ADB daemon is starting / device handshake in progress
    Connecting(String),
    /// Device connected and authorized — shows device model
    Connected(String),
    /// An error occurred (message included)
    Error(String),
}

impl AdbStatus {
    /// Human-readable status text shown in the status bar
    pub fn text(&self) -> String {
        match self {
            AdbStatus::Disconnected => "No device connected".to_string(),
            AdbStatus::Unauthorized => {
                "Device found — check your phone screen and tap Allow".to_string()
            }
            AdbStatus::Connecting(name) => format!("Connecting to {}…", name),
            AdbStatus::Connected(name) => format!("Connected: {}", name),
            AdbStatus::Error(msg) => format!("Error: {}", msg),
        }
    }
}

impl Default for AdbStatus {
    fn default() -> Self {
        AdbStatus::Disconnected
    }
}

/// Status bar rendered at the bottom of the application window
#[derive(Debug, Clone)]
pub struct StatusBar {
    theme: ThemeColors,
}

impl StatusBar {
    pub fn new(theme: ThemeColors) -> Self {
        Self { theme }
    }

    /// Render the status bar, coloring the status text based on connection state.
    pub fn view<'a, Message: 'a + Clone>(&'a self, status: &AdbStatus) -> Element<'a, Message> {
        let theme = self.theme;

        let status_color = match status {
            AdbStatus::Disconnected => theme.text_secondary,
            AdbStatus::Unauthorized => theme.warning,
            AdbStatus::Connecting(_) => theme.accent,
            AdbStatus::Connected(_) => theme.success,
            AdbStatus::Error(_) => theme.error,
        };

        let content = row![text(status.text()).size(12).color(status_color)]
            .padding([6, 15])
            .spacing(20);

        container(content)
            .width(Fill)
            .height(STATUS_BAR_HEIGHT)
            .style(move |_theme| container::Style {
                background: Some(theme.background_secondary.into()),
                border: Border {
                    color: theme.border,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }
}
