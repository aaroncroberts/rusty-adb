//! Shared view helper functions for file browser panes.

use crate::theme::ThemeColors;
use iced::widget::{button, column, container, text};
use iced::{Element, Fill};

/// Error state body — shown when `list_dir` fails.
pub fn view_error<'a, Message: 'a + Clone>(
    theme: ThemeColors,
    msg: &str,
    on_retry: Option<Message>,
) -> Element<'a, Message> {
    let mut content = column![
        text("Error").size(13).color(theme.text_secondary),
        text(msg.to_string()).size(11).color(theme.error),
    ]
    .spacing(6);

    if let Some(retry_msg) = on_retry {
        content = content.push(
            button(text("Retry").size(11).color(theme.text))
                .padding([4, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(theme.background_secondary.into()),
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(retry_msg),
        );
    }

    container(content)
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        })
        .into()
}

/// No-device guide — shown when no Android device is connected.
///
/// Moved here from `android_pane.rs` since it has no backend-specific logic.
pub fn view_connect_guide<'a, Message: 'a + Clone>(theme: ThemeColors) -> Element<'a, Message> {
    let guide = column![
        text("No Android device connected").size(13).color(theme.text_secondary),
        text("").size(6),
        text("1. Enable USB Debugging on your device").size(11).color(theme.text_secondary),
        text("2. Connect via USB").size(11).color(theme.text_secondary),
        text("3. Tap  Allow  when prompted").size(11).color(theme.text_secondary),
    ]
    .spacing(4);

    container(guide)
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        })
        .into()
}

