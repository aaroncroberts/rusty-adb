//! Banner view implementations: error, toast, and hero banners.

use crate::{App, Message};
use iced::widget::{button, column, container, row, text};
use iced::{Border, Element, Fill};

impl App {
    pub(super) fn view_error_banner<'a>(&'a self, msg: &'a str) -> Element<'a, Message> {
        let t = self.theme;
        let content = row![
            text(format!("[!] {msg}")).size(12).color(t.error),
            iced::widget::Space::with_width(Fill),
            button(text("X").size(11).color(t.error))
                .style(t.transparent_button())
                .on_press(Message::DismissError),
        ]
        .align_y(iced::Alignment::Center)
        .padding([4, 12]);

        container(content)
            .width(Fill)
            .style(t.error_banner())
            .into()
    }

    pub(super) fn view_toast_banner<'a>(&'a self, msg: &'a str) -> Element<'a, Message> {
        let t = self.theme;
        let content = row![text(msg).size(12).color(t.text).width(Fill),].padding([6, 15]);
        container(content)
            .width(Fill)
            .style(t.success_banner())
            .into()
    }

    /// Retro ASCII hero banner — a slim persistent strip above the pane menu bars.
    ///
    /// Shows: ASCII logotype · version · tagline, in phosphor-green palette.
    pub(super) fn view_hero_banner<'a>(&self) -> Element<'a, Message> {
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
}
