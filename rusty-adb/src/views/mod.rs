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

use crate::{App, Message};
use std::path::PathBuf;
use crate::adb::AdbClient;
use crate::fs::AndroidContext;
use crate::file_pane::RenameCbs;
use crate::adb::AdbStatus;
use crate::ViewMode;
use iced::widget::{
    button, column, container, row, stack, text, vertical_rule,
};
use iced::{Border, Element, Fill};

impl App {
    pub(super) fn view(&self) -> Element<Message> {
        // ADB not installed — replace panes with the setup guide
        if self.adb_status == AdbStatus::NotFound {
            let mut items: Vec<Element<Message>> = vec![self.view_header()];
            items.push(self.view_adb_not_found());
            items.push(self.status_bar.view(&self.adb_status, None, None));
            return column(items).into();
        }

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
            ),
        );

        let base: Element<Message> = column(items).into();

        // Stack-based modal overlays (log viewer > settings > about > preview)
        if self.log_viewer_open {
            stack![base, self.view_log_viewer()].into()
        } else if self.settings_open {
            stack![base, self.view_settings_modal()].into()
        } else if self.about_open {
            stack![base, self.view_about_modal()].into()
        } else if let Some(modal_content) = &self.preview_modal {
            stack![base, self.view_preview_modal(modal_content)].into()
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
            self.local_pane.entries.get(i).map(|e| !e.is_dir).unwrap_or(false)
        });
        let android_sel_has_files = self.android_pane.selected.iter().any(|&i| {
            self.android_pane.entries.get(i).map(|e| !e.is_dir).unwrap_or(false)
        });
        let android_sel_single =
            self.android_pane.selected.len() == 1 && self.android_pane.rename_pending.is_none();
        let android_sel_nonempty =
            !self.android_pane.selected.is_empty() && self.android_pane.rename_pending.is_none();

        // ── Local pane ────────────────────────────────────────────────────────
        let local_menu = self.view_pane_menu_bar(
            false,
            self.local_view_mode,
            self.local_pane.show_hidden,
            local_sel_has_files,
            android_sel_has_files,
            android_sel_single,
            android_sel_nonempty,
            has_device,
            no_transfer,
        );

        let local_content: Element<Message> = match self.local_view_mode {
            // Name-only rows with type icons — compact, maximum density
            ViewMode::List => self.local_pane.view_list(
                &(),
                self.theme,
                Message::LocalNavigateTo,
                Message::LocalSelectEntry,
                Message::LocalSortBy,
                None,
            ),
            // Finder-style column view: path ancestors on left, entries on right
            ViewMode::Details => self.view_columns_impl(false),
            ViewMode::Grid => self.view_local_grid(),
            ViewMode::Icon => self.view_local_icon(),
        };

        let local_path = self.local_pane.current_path.clone();
        let local_title = self.view_pane_title_bar("LOCAL", &local_path, Message::LocalNavigateTo);

        let left: Element<Message> = column![local_title, local_menu, local_content]
            .width(Fill)
            .height(Fill)
            .into();

        // ── Android pane ──────────────────────────────────────────────────────
        let android_menu = self.view_pane_menu_bar(
            true,
            self.android_view_mode,
            self.android_pane.show_hidden,
            local_sel_has_files,
            android_sel_has_files,
            android_sel_single,
            android_sel_nonempty,
            has_device,
            no_transfer,
        );

        let default_android_ctx = AndroidContext {
            client: AdbClient { adb_path: PathBuf::new() },
            serial: String::new(),
            storage_roots: Vec::new(),
        };
        let android_ctx_ref = self.android_ctx.as_ref().unwrap_or(&default_android_ctx);

        let android_content: Element<Message> = match self.android_view_mode {
            // Name-only rows with type icons
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
            // Finder-style column view: path ancestors on left, entries on right
            ViewMode::Details => self.view_columns_impl(true),
            ViewMode::Grid => self.view_android_grid(),
            ViewMode::Icon => self.view_android_icon(),
        };

        // Wrap android content with drop-zone highlight
        let hover = self.file_hover_active;
        let android_content: Element<Message> = container(android_content)
            .width(Fill)
            .height(Fill)
            .style(move |_theme| container::Style {
                border: Border {
                    color: if hover {
                        t.accent.scale_alpha(0.8)
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if hover { 2.0 } else { 0.0 },
                    ..Default::default()
                },
                ..Default::default()
            })
            .into();

        let android_path = self.android_pane.current_path.clone();
        let android_title = self.view_pane_title_bar("ANDROID", &android_path, Message::AndroidNavigateTo);

        let right: Element<Message> = column![android_title, android_menu, android_content]
            .width(Fill)
            .height(Fill)
            .into();

        let divider = container(vertical_rule(1))
            .height(Fill)
            .style(move |_theme| container::Style {
                background: Some(t.border.into()),
                ..Default::default()
            });

        row![left, divider, right].width(Fill).height(Fill).into()
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
/// Shared by `modals` submodule — called as `super::modal_backdrop(...)`.
pub(super) fn modal_backdrop<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        container(content)
            .center_x(Fill)
            .center_y(Fill)
            .width(Fill)
            .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(|_th| container::Style {
        background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6).into()),
        ..Default::default()
    })
    .into()
}
