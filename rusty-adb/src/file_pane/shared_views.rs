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
/// Walks the user through enabling Developer Mode and USB Debugging,
/// connecting the cable, and authorising the host computer.
/// No-device guide — shown when no Android device is connected.
///
/// Walks the user through enabling Developer Mode, USB Debugging,
/// connecting the cable, and authorising the host computer.
pub fn view_connect_guide<'a, Message: 'a + Clone>(theme: ThemeColors) -> Element<'a, Message> {
    let t = theme;
    let h = |s: &'static str| text(s).size(12).color(t.text);
    let s = |s: &'static str| text(s).size(11).color(t.text_secondary);
    let a = |s: &'static str| text(s).size(11).color(t.accent).font(iced::Font::MONOSPACE);
    let gap = || text("").size(6);

    let guide = column![
        text("No Android Device Connected")
            .size(18)
            .color(t.accent)
            .font(iced::Font::MONOSPACE),
        text("Connect a device via USB and follow the steps below.")
            .size(12)
            .color(t.text_secondary),
        gap(),
        gap(),
        h("Step 1 - Unlock Developer Options"),
        a("  Settings > About Phone > Build Number  (tap 7 times)"),
        s("  Samsung:  About Phone > Software Information > Build Number"),
        s("  Xiaomi:   About Phone > All Specs > MIUI Version"),
        s("  OnePlus:  About Device > Version > Build Number"),
        s("  Your PIN may be required. \"You are now a developer!\" confirms success."),
        gap(),
        h("Step 2 - Enable USB Debugging"),
        a("  Settings > Developer Options > USB Debugging  (toggle on)"),
        gap(),
        h("Step 3 - Connect via USB"),
        s("  Plug in USB. Swipe the notification shade, tap the USB notification,"),
        s("  and choose  File Transfer / MTP."),
        gap(),
        h("Step 4 - Authorise this computer"),
        s("  Tap Allow on the \"Allow USB debugging?\" dialog."),
        s("  Tick \"Always allow from this computer\" to skip this next time."),
        gap(),
        h("Troubleshooting"),
        s("  No dialog? Disconnect, toggle USB Debugging off/on, reconnect."),
        s("  Shows \"unauthorized\"? Revoke all authorisations in Developer Options,"),
        s("  reconnect, and tap Allow again."),
    ]
    .spacing(3)
    .width(480);

    container(
        iced::widget::scrollable(container(guide).padding([24, 28]))
            .width(Fill)
            .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(move |_t| container::Style {
        background: Some(t.background.into()),
        ..Default::default()
    })
    .into()
}
