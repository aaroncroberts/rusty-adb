//! View implementations for `App` — module root.
//!
//! Top-level methods: `view`, `view_toolbar`, `view_panes`, `view_delete_confirm`.
//! Shared free-function helper: `modal_backdrop` (used by modals submodule).

mod banners;
mod modals;
mod pane_controls;
mod rendering;
mod setup;
pub mod status_bar;

use crate::adb::AdbClient;
use crate::adb::AdbStatus;
use crate::file_pane::RenameCbs;
use crate::fs::AndroidContext;
use crate::icons;
use crate::ViewMode;
use crate::{App, Message, PaneLayout};
use iced::widget::tooltip::Position as TipPos;
use iced::widget::{
    button, column, container, mouse_area, row, stack, text, tooltip, vertical_rule,
};
use iced::{Border, Element, Fill, FillPortion};
use pane_controls::PaneMenuState;
use std::path::PathBuf;

impl App {
    pub(super) fn view(&self) -> Element<Message> {
        // ADB not installed — replace panes with the setup guide
        if self.adb_status == AdbStatus::NotFound {
            let mut items: Vec<Element<Message>> = vec![self.view_header()];
            items.push(self.view_adb_not_found());
            items.push(self.status_bar.view(
                &self.adb_status,
                None,
                None,
                Some(Message::OpenLogViewer),
                None,
                None,
            ));
            return column(items).into();
        }

        let queue_summary = {
            let s = self.copy_queue.summary();
            let text = s.status_text();
            if text.is_empty() { None } else { Some(text) }
        };

        let mut items: Vec<Element<Message>> = vec![self.view_header()];
        if let Some(msg) = &self.error_banner {
            items.push(self.view_error_banner(msg));
        }
        if let Some(msg) = &self.toast {
            items.push(self.view_toast_banner(msg));
        }
        if let Some(paths) = &self.delete_confirm_paths {
            items.push(self.view_delete_confirm(paths));
        }
        items.push(self.view_panes());
        items.push(
            self.status_bar.view(
                &self.adb_status,
                self.transfer_status.as_ref(),
                self.active_transfer
                    .as_ref()
                    .map(|_| Message::CancelTransfer),
                Some(Message::OpenLogViewer),
                queue_summary,
                Some(Message::OpenDeviceDetails),
            ),
        );

        let base: Element<Message> = column(items).into();

        // Stack-based modal overlays (log viewer > settings > about > device details > preview > queue > copy-confirm)
        if self.log_viewer_open {
            stack![base, self.view_log_viewer()].into()
        } else if self.settings_open {
            stack![base, self.view_settings_modal()].into()
        } else if self.about_open {
            stack![base, self.view_about_modal()].into()
        } else if self.device_details_open {
            stack![base, self.view_device_details_modal()].into()
        } else if let Some(modal_content) = &self.preview_modal {
            stack![base, self.view_preview_modal(modal_content)].into()
        } else if self.queue_open {
            stack![base, self.view_queue_dialog()].into()
        } else if self.copy_confirm_open {
            stack![base, self.view_copy_confirm()].into()
        } else {
            base
        }
    }

    fn view_panes(&self) -> Element<Message> {
        let t = self.theme;
        let has_device = self.active_serial.is_some();
        let no_transfer = self.active_transfer.is_none();

        // ── Precompute selection flags ────────────────────────────────────────
        let local_sel_has_files = self.local_pane.selected.iter().any(|&i| {
            self.local_pane
                .entries
                .get(i)
                .map(|e| !e.is_dir)
                .unwrap_or(false)
        });
        let android_sel_has_files = self.android_pane.selected.iter().any(|&i| {
            self.android_pane
                .entries
                .get(i)
                .map(|e| !e.is_dir)
                .unwrap_or(false)
        });
        let android_sel_single =
            self.android_pane.selected.len() == 1 && self.android_pane.rename_pending.is_none();
        let android_sel_nonempty =
            !self.android_pane.selected.is_empty() && self.android_pane.rename_pending.is_none();

        let divider = container(vertical_rule(1))
            .height(Fill)
            .style(move |_theme| container::Style {
                background: Some(t.border.into()),
                ..Default::default()
            });

        // ── Narrow sidebar helper — shown when the opposite pane is expanded ──
        // Narrow sidebar — fixed width, just enough for the two action buttons.
        // Label is rendered as vertical letters stacked in a small column above
        // the buttons so the sidebar stays as thin as possible.
        let sidebar = |label: &'static str, is_android: bool| -> Element<Message> {
            let expand_msg = Message::ExpandPane(is_android);
            // >> = expand this pane, >< = restore equal split
            let expand_btn = tooltip(
                button(
                    text(icons::expand())
                        .size(14)
                        .color(t.accent)
                        .font(icons::font()),
                )
                .style(t.transparent_button())
                .padding([4, 4])
                .on_press(expand_msg),
                text("Expand this pane").size(11),
                TipPos::Right,
            );
            let restore_btn = tooltip(
                button(
                    text(icons::split())
                        .size(14)
                        .color(t.text_secondary)
                        .font(icons::font()),
                )
                .style(t.transparent_button())
                .padding([4, 4])
                .on_press(Message::CollapsePanes),
                text("Restore equal split").size(11),
                TipPos::Right,
            );
            // Stack label chars vertically, centred in remaining space
            let label_col = label.chars().fold(
                iced::widget::Column::new()
                    .spacing(2)
                    .align_x(iced::Alignment::Center),
                |col, ch| {
                    col.push(
                        text(ch.to_string())
                            .size(11)
                            .color(t.text_secondary)
                            .font(iced::Font::MONOSPACE),
                    )
                },
            );
            let label_area = container(label_col)
                .width(Fill)
                .height(Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center);
            container(
                column![restore_btn, expand_btn, label_area]
                    .spacing(2)
                    .align_x(iced::Alignment::Center)
                    .padding([6, 0]),
            )
            .width(iced::Length::Fixed(46.0))
            .height(Fill)
            .style(t.secondary_panel())
            .into()
        };

        match self.pane_layout {
            // ── Equal split (default) ─────────────────────────────────────────
            PaneLayout::Split => {
                let local_menu = self.view_pane_menu_bar(
                    false,
                    self.local_view_mode,
                    self.local_pane.show_hidden,
                    PaneMenuState {
                        local_sel_has_files,
                        android_sel_has_files,
                        android_sel_single,
                        android_sel_nonempty,
                        has_device,
                        no_transfer,
                    },
                );
                let local_content = self.build_local_content();
                let local_path = self.local_pane.current_path.clone();
                let local_title = self.view_pane_title_bar(
                    "LOCAL",
                    &local_path,
                    false,
                    self.pane_layout,
                    Message::LocalNavigateTo,
                );
                let left: Element<Message> = column![local_title, local_menu, local_content]
                    .width(Fill)
                    .height(Fill)
                    .into();

                let android_menu = self.view_pane_menu_bar(
                    true,
                    self.android_view_mode,
                    self.android_pane.show_hidden,
                    PaneMenuState {
                        local_sel_has_files,
                        android_sel_has_files,
                        android_sel_single,
                        android_sel_nonempty,
                        has_device,
                        no_transfer,
                    },
                );
                let android_content = self.build_android_content();
                let android_path = self.android_pane.current_path.clone();
                let android_title = self.view_pane_title_bar(
                    "ANDROID",
                    &android_path,
                    true,
                    self.pane_layout,
                    Message::AndroidNavigateTo,
                );
                let right: Element<Message> = column![android_title, android_menu, android_content]
                    .width(Fill)
                    .height(Fill)
                    .into();

                row![left, divider, right].width(Fill).height(Fill).into()
            }

            // ── Local pane expanded; Android sidebar on right ─────────────────
            PaneLayout::LocalExpanded => {
                let local_menu = self.view_pane_menu_bar(
                    false,
                    self.local_view_mode,
                    self.local_pane.show_hidden,
                    PaneMenuState {
                        local_sel_has_files,
                        android_sel_has_files,
                        android_sel_single,
                        android_sel_nonempty,
                        has_device,
                        no_transfer,
                    },
                );
                let local_content = self.build_local_content();
                let local_path = self.local_pane.current_path.clone();
                let local_title = self.view_pane_title_bar(
                    "LOCAL",
                    &local_path,
                    false,
                    self.pane_layout,
                    Message::LocalNavigateTo,
                );
                let left: Element<Message> = column![local_title, local_menu, local_content]
                    .width(FillPortion(5))
                    .height(Fill)
                    .into();

                row![left, divider, sidebar("ANDROID", true)]
                    .width(Fill)
                    .height(Fill)
                    .into()
            }

            // ── Android pane expanded; local sidebar on left ──────────────────
            PaneLayout::AndroidExpanded => {
                let android_menu = self.view_pane_menu_bar(
                    true,
                    self.android_view_mode,
                    self.android_pane.show_hidden,
                    PaneMenuState {
                        local_sel_has_files,
                        android_sel_has_files,
                        android_sel_single,
                        android_sel_nonempty,
                        has_device,
                        no_transfer,
                    },
                );
                let android_content = self.build_android_content();
                let android_path = self.android_pane.current_path.clone();
                let android_title = self.view_pane_title_bar(
                    "ANDROID",
                    &android_path,
                    true,
                    self.pane_layout,
                    Message::AndroidNavigateTo,
                );
                let right: Element<Message> = column![android_title, android_menu, android_content]
                    .width(FillPortion(5))
                    .height(Fill)
                    .into();

                row![sidebar("LOCAL", false), divider, right]
                    .width(Fill)
                    .height(Fill)
                    .into()
            }
        }
    }

    /// Build the local pane content element for the current view mode.
    fn build_local_content(&self) -> Element<Message> {
        let content = match self.local_view_mode {
            ViewMode::List => self.local_pane.view_list(
                &(),
                self.theme,
                Message::LocalNavigateTo,
                Message::LocalSelectEntry,
                Message::LocalSortBy,
                None,
            ),
            ViewMode::Details => self.view_columns_impl(false),
            ViewMode::Grid => self.view_local_grid(),
            ViewMode::Icon => self.view_local_icon(),
        };

        // When items are selected, wrap with mouse_area so pressing and moving
        // toward the android pane starts an in-app drag.
        let has_selection = !self.local_pane.selected.is_empty();
        if has_selection {
            mouse_area(content)
                .on_press(Message::LocalDragStarted)
                .into()
        } else {
            content
        }
    }

    /// Build the android pane content element for the current view mode.
    fn build_android_content(&self) -> Element<Message> {
        let default_android_ctx = AndroidContext {
            client: AdbClient {
                adb_path: PathBuf::new(),
            },
            serial: String::new(),
            storage_roots: Vec::new(),
        };
        let android_ctx_ref = self.android_ctx.as_ref().unwrap_or(&default_android_ctx);

        let content: Element<Message> = match self.android_view_mode {
            ViewMode::List => self.android_pane.view_list(
                android_ctx_ref,
                self.theme,
                Message::AndroidNavigateTo,
                Message::AndroidSelectEntry,
                Message::AndroidSortBy,
                Some(RenameCbs {
                    on_input: Box::new(Message::AndroidRenameInput),
                    on_commit: Message::AndroidRenameCommit,
                }),
            ),
            ViewMode::Details => self.view_columns_impl(true),
            ViewMode::Grid => self.view_android_grid(),
            ViewMode::Icon => self.view_android_icon(),
        };

        // Wrap with drop-zone highlight — active for both OS drops and in-app drags
        let hover = self.file_hover_active;
        let in_app_drag = self.drag_in_progress;
        let t = self.theme;
        let drop_active = hover || in_app_drag;
        let styled = container(content)
            .width(Fill)
            .height(Fill)
            .style(move |_theme| container::Style {
                border: Border {
                    color: if drop_active {
                        t.accent.scale_alpha(0.8)
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if drop_active { 2.0 } else { 0.0 },
                    ..Default::default()
                },
                ..Default::default()
            });

        // When an in-app drag is in progress, catch the mouse release to complete the drop
        if in_app_drag {
            mouse_area(styled)
                .on_release(Message::DroppedOnAndroid)
                .into()
        } else {
            styled.into()
        }
    }

    /// Delete confirmation banner — shown above the panes when paths are pending deletion.
    fn view_delete_confirm<'a>(&'a self, paths: &'a [PathBuf]) -> Element<'a, Message> {
        let t = self.theme;
        let label = if paths.len() == 1 {
            format!(
                "Delete \"{}\"? This cannot be undone.",
                paths[0].file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!("Delete {} items? This cannot be undone.", paths.len())
        };

        let confirm_btn = button(text("Confirm Delete").size(12).color(iced::Color::WHITE))
            .style(t.error_button())
            .padding([4, 12])
            .on_press(Message::AndroidDeleteConfirm);

        let cancel_btn = button(text("Cancel").size(12).color(t.text))
            .style(t.transparent_button())
            .padding([4, 8])
            .on_press(Message::AndroidDeleteCancel);

        let content = row![
            text(label).size(12).color(t.text).width(Fill),
            confirm_btn,
            cancel_btn,
        ]
        .align_y(iced::Alignment::Center)
        .spacing(8)
        .padding([4, 12]);

        container(content)
            .width(Fill)
            .style(t.warning_banner())
            .into()
    }
}

/// Semi-transparent full-window backdrop used by all modal overlays.
///
/// Wraps the backdrop in a `mouse_area` so pointer events are consumed and
/// cannot reach the underlying pane content while a modal is open.
/// Shared by `modals` submodule — called as `super::modal_backdrop(...)`.
pub(super) fn modal_backdrop<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let backdrop = container(
        container(content)
            .center_x(Fill)
            .center_y(Fill)
            .width(Fill)
            .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(|_th| container::Style {
        // Dark teal-tinted overlay — reinforces the theme colour while
        // visually pushing the pane content into the background.
        background: Some(iced::Color::from_rgba(0.04, 0.12, 0.11, 0.78).into()),
        ..Default::default()
    });

    // mouse_area absorbs all pointer events so clicks cannot pass through
    // the overlay to the file panes beneath.
    mouse_area(backdrop).on_press(Message::EscapePressed).into()
}
