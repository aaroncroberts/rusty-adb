//! Status bar component for rusty-adb
//!
//! Renders a horizontal bar at the bottom of the window showing:
//! - ADB connection state (when idle)
//! - File transfer progress bar + speed (when a transfer is active)

use crate::theme::ThemeColors;
use iced::widget::{button, container, progress_bar, row, text};
use iced::{Border, Element, Fill};

/// Height of the status bar in pixels
pub const STATUS_BAR_HEIGHT: f32 = 30.0;

/// ADB device connection status
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AdbStatus {
    /// ADB binary not found on this system — install screen is shown
    NotFound,
    /// No device connected
    #[default]
    Disconnected,
    /// Device found but user hasn't authorized USB debugging yet
    Unauthorized,
    /// ADB daemon is starting / device handshake in progress (reserved for future use)
    #[allow(dead_code)]
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
            AdbStatus::NotFound => "ADB not installed — follow the setup guide".to_string(),
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


// ─── Transfer status ───────────────────────────────────────────────────────────

/// Live transfer state shown in the status bar during an active copy
#[derive(Debug, Clone)]
pub struct TransferStatus {
    /// Display name of the file being transferred
    pub filename: String,
    /// Progress 0–100
    pub percent: u8,
    /// Human-readable speed e.g. "12.3 MB/s" (empty until adb reports it)
    pub speed_display: String,
    /// 1-based index of the current job in the queue
    pub job_index: usize,
    /// Total jobs in the queue
    pub job_total: usize,
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_disconnected() {
        assert_eq!(AdbStatus::Disconnected.text(), "No device connected");
    }

    #[test]
    fn text_unauthorized() {
        assert!(AdbStatus::Unauthorized.text().contains("Allow"));
    }

    #[test]
    fn text_connecting() {
        let s = AdbStatus::Connecting("Pixel 7".to_string()).text();
        assert!(s.contains("Pixel 7"), "expected device name in: {}", s);
    }

    #[test]
    fn text_connected() {
        let s = AdbStatus::Connected("Pixel 7".to_string()).text();
        assert!(s.contains("Pixel 7") && s.contains("Connected"));
    }

    #[test]
    fn text_error() {
        let s = AdbStatus::Error("timeout".to_string()).text();
        assert!(s.contains("Error") && s.contains("timeout"));
    }
}

// ─── Widget ────────────────────────────────────────────────────────────────────

/// Status bar rendered at the bottom of the application window
#[derive(Debug, Clone)]
pub struct StatusBar {
    theme: ThemeColors,
}

impl StatusBar {
    pub fn new(theme: ThemeColors) -> Self {
        Self { theme }
    }

    /// Render the status bar.
    ///
    /// When `transfer` is `Some`, shows a progress bar + speed instead of
    /// the connection status text. `on_cancel` enables the Cancel button.
    pub fn view<'a, Message: 'a + Clone>(
        &'a self,
        status: &AdbStatus,
        transfer: Option<&TransferStatus>,
        on_cancel: Option<Message>,
    ) -> Element<'a, Message> {
        let theme = self.theme;

        let inner: Element<Message> = if let Some(xfer) = transfer {
            // ── Transfer in progress ──────────────────────────────────────────
            let bar = progress_bar(0.0..=100.0, xfer.percent as f32)
                .height(8)
                .style(move |_theme| iced::widget::progress_bar::Style {
                    background: theme.border.into(),
                    bar: theme.accent.into(),
                    border: iced::Border::default(),
                });

            let queue_label = if xfer.job_total > 1 {
                format!("[{}/{}]  ", xfer.job_index, xfer.job_total)
            } else {
                String::new()
            };

            let label = if xfer.speed_display.is_empty() {
                format!("  {}{}  {}%", queue_label, xfer.filename, xfer.percent)
            } else {
                format!("  {}{}  {}%  {}", queue_label, xfer.filename, xfer.percent, xfer.speed_display)
            };

            let cancel_btn = button(text("✕ Cancel").size(11).color(theme.error))
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });
            let cancel_btn = if let Some(msg) = on_cancel {
                cancel_btn.on_press(msg)
            } else {
                cancel_btn
            };

            row![bar, text(label).size(12).color(theme.text_secondary), cancel_btn]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .into()
        } else {
            // ── Connection status ─────────────────────────────────────────────
            let status_color = match status {
                AdbStatus::NotFound => theme.warning,
                AdbStatus::Disconnected => theme.text_secondary,
                AdbStatus::Unauthorized => theme.warning,
                AdbStatus::Connecting(_) => theme.accent,
                AdbStatus::Connected(_) => theme.success,
                AdbStatus::Error(_) => theme.error,
            };
            text(status.text()).size(12).color(status_color).into()
        };

        let content = row![inner].padding([6, 15]).spacing(20);

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
