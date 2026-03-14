//! Status bar widget — renders ADB connection state and transfer progress.

use crate::adb::{AdbStatus, TransferStatus};
use crate::theme::ThemeColors;
use iced::widget::{button, container, progress_bar, row, text};
use iced::{Border, Element, Fill};

/// Height of the status bar in pixels
pub const STATUS_BAR_HEIGHT: f32 = 30.0;

/// Status bar rendered at the bottom of the application window.
///
/// Delegates all state to its callers — owns only the theme needed for styling.
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
                format!(
                    "  {}{}  {}%  {}",
                    queue_label, xfer.filename, xfer.percent, xfer.speed_display
                )
            };

            let cancel_btn =
                button(text("X Cancel").size(11).color(theme.error)).style(move |_t, _s| {
                    button::Style {
                        background: None,
                        ..Default::default()
                    }
                });
            let cancel_btn = if let Some(msg) = on_cancel {
                cancel_btn.on_press(msg)
            } else {
                cancel_btn
            };

            row![
                bar,
                text(label).size(12).color(theme.text_secondary),
                cancel_btn
            ]
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
