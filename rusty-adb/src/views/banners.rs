//! Banner view implementations: error, toast, and hero banners.

use crate::icons;
use crate::{App, Message};
use iced::widget::tooltip::Position as TipPos;
use iced::widget::{button, column, container, row, text, tooltip};
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

    /// Combined header row: toolbar buttons on the left (2/3), banner on the right (1/3).
    ///
    /// Replaces the former two-row layout (`view_hero_banner` + `view_toolbar`).
    /// - Left portion: Settings and About buttons, left-aligned.
    /// - Right portion: ASCII logotype + tagline, right-aligned.
    pub(crate) fn view_header<'a>(&self) -> Element<'a, Message> {
        let t = self.theme;
        let version = env!("CARGO_PKG_VERSION");

        // ── Right: banner logo + tagline ──────────────────────────────────
        let logo = text(format!("◄◄ RUSTY-ADB ►► v{}", version))
            .size(13)
            .color(t.accent)
            .font(iced::Font::MONOSPACE);

        let tagline = text("Android file manager · Rust + Iced")
            .size(9)
            .color(t.text_secondary)
            .font(iced::Font::MONOSPACE);

        let banner_col = column![logo, tagline]
            .spacing(1)
            .align_x(iced::Alignment::End);

        // Clicking the banner opens the About dialog
        let banner_btn = tooltip(
            button(banner_col)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .padding([4, 12])
                .on_press(Message::OpenAbout),
            text("About rusty-adb").size(11),
            TipPos::Bottom,
        );

        let right = container(banner_btn)
            .width(Fill)
            .align_x(iced::Alignment::End);

        // ── Left: toolbar buttons ─────────────────────────────────────────
        let settings_btn = button(
            text(icons::settings()).font(icons::font()).size(14).color(t.text),
        )
        .padding([4, 10])
        .style(t.transparent_button())
        .on_press(Message::OpenSettings);

        let left = container(
            row![settings_btn]
                .spacing(8)
                .padding([0, 8])
                .align_y(iced::Alignment::Center),
        )
        .width(iced::Length::Shrink);

        // ── Combined row ──────────────────────────────────────────────────
        container(row![left, right].align_y(iced::Alignment::Center))
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
