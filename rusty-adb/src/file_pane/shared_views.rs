//! Shared view helper functions for file browser panes.

use crate::theme::ThemeColors;
use iced::widget::{button, column, container, row, text};
use iced::{Border, Element, Fill};
use std::path::PathBuf;

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

/// Render a clickable breadcrumb bar for `current_path`.
///
/// Produces a row like:  `/` › `Users` › `aaron` › `Documents`
///
/// Each segment except the last emits `on_navigate(ancestor_path)` when clicked.
/// Messages are computed eagerly so the element lifetime is independent of the caller.
pub fn view_breadcrumb<'a, Message: 'a + Clone>(
    current_path: &std::path::Path,
    theme: ThemeColors,
    on_navigate: impl Fn(PathBuf) -> Message,
) -> iced::widget::Container<'a, Message> {
    let mut segments: Vec<(String, PathBuf)> = current_path
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| {
            let label = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".to_string());
            (label, p.to_path_buf())
        })
        .collect();

    if segments.first().map(|(l, _)| l.as_str()) != Some("/") {
        segments.insert(0, ("/".to_string(), PathBuf::from("/")));
    }

    let last_idx = segments.len().saturating_sub(1);
    let mut crumb_row: Vec<iced::Element<'a, Message>> = Vec::new();

    for (i, (label, path)) in segments.into_iter().enumerate() {
        let is_last = i == last_idx;
        if is_last {
            crumb_row.push(text(label).size(11).color(theme.accent).into());
        } else {
            let nav_msg = on_navigate(path);
            let btn = button(text(label).size(11).color(theme.text_secondary))
                .style(move |_t, _s| button::Style { background: None, ..Default::default() })
                .padding([0, 2])
                .on_press(nav_msg);
            crumb_row.push(btn.into());
            crumb_row.push(text("›").size(11).color(theme.text_secondary).into());
        }
    }

    let content = row(crumb_row)
        .align_y(iced::Alignment::Center)
        .spacing(2)
        .padding([2, 8]);

    container(content)
        .width(Fill)
        .height(22.0)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            border: Border { color: theme.border, width: 1.0, ..Default::default() },
            ..Default::default()
        })
}
