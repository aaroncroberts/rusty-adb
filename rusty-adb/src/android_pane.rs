//! Android device file browser pane
//!
//! Renders the right pane: directory listing of an Android device via ADB.
//!
//! # State machine
//! ```text
//! NoDevice ──(device connected)──► Loading ──(entries arrive)──► Browsing
//!                                     │                              │
//!                                     └──(adb error)──► Error        │
//!                                                                    ▼
//!                                                           (click directory)
//!                                                                    │
//!                                                                 Loading
//! ```

use std::path::PathBuf;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Border, Color, Element, Fill};

use crate::adb::AndroidEntry;
use crate::theme::ThemeColors;

// ─── Pane State ───────────────────────────────────────────────────────────────

/// What is the Android pane currently showing?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AndroidPaneState {
    /// No authorized device connected
    NoDevice,
    /// Fetching directory listing from the device (non-blocking)
    Loading,
    /// Directory listing is ready
    Browsing,
    /// ADB returned an error for the last request
    Error(String),
}

/// State for the Android device file browser pane
#[derive(Debug, Clone)]
pub struct AndroidPane {
    pub state: AndroidPaneState,
    /// Current path on the device (starts at `/sdcard`)
    pub current_path: PathBuf,
    /// Entries returned by the last successful `adb shell ls -la`
    pub entries: Vec<AndroidEntry>,
    /// Selected entry indices (multi-select)
    pub selected: Vec<usize>,
    /// Storage roots discovered on the connected device
    pub storage_roots: Vec<PathBuf>,
    /// Spinner frame counter (incremented by SpinnerTick messages)
    pub spinner_frame: u8,
}

impl Default for AndroidPane {
    fn default() -> Self {
        Self {
            state: AndroidPaneState::NoDevice,
            current_path: PathBuf::from("/sdcard"),
            entries: Vec::new(),
            selected: Vec::new(),
            storage_roots: Vec::new(),
            spinner_frame: 0,
        }
    }
}

impl AndroidPane {
    /// Called when a device connects — begin loading /sdcard.
    pub fn on_device_connected(&mut self) {
        tracing::info!("android pane: device connected, transitioning to Loading");
        self.current_path = PathBuf::from("/sdcard");
        self.entries.clear();
        self.selected.clear();
        self.state = AndroidPaneState::Loading;
    }

    /// Called when the device disconnects.
    pub fn on_device_disconnected(&mut self) {
        tracing::info!("android pane: device disconnected, transitioning to NoDevice");
        self.entries.clear();
        self.selected.clear();
        self.storage_roots.clear();
        self.state = AndroidPaneState::NoDevice;
    }

    /// Called to start navigating to a new path.
    pub fn begin_navigate(&mut self, path: PathBuf) {
        tracing::info!(
            from = %self.current_path.display(),
            to = %path.display(),
            "android pane: navigating"
        );
        self.current_path = path;
        self.selected.clear();
        self.state = AndroidPaneState::Loading;
    }

    /// Called when entries arrive from ADB.
    pub fn on_entries_loaded(
        &mut self,
        path: PathBuf,
        mut entries: Vec<AndroidEntry>,
        roots: Vec<PathBuf>,
    ) {
        // Discard stale responses (user may have navigated elsewhere)
        if self.current_path != path {
            tracing::debug!(
                expected = %self.current_path.display(),
                arrived = %path.display(),
                "android pane: discarding stale directory response"
            );
            return;
        }
        // Dirs first, then alphabetical
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        tracing::info!(
            path = %path.display(),
            count = entries.len(),
            roots = roots.len(),
            "android pane: directory loaded, transitioning to Browsing"
        );
        self.entries = entries;
        self.storage_roots = roots;
        self.state = AndroidPaneState::Browsing;
    }

    /// Called when ADB returns an error for the current path.
    pub fn on_error(&mut self, msg: String) {
        tracing::warn!(
            path = %self.current_path.display(),
            error = %msg,
            "android pane: directory load error, transitioning to Error"
        );
        self.state = AndroidPaneState::Error(msg);
    }

    /// Toggle selection of an entry by index (multi-select).
    pub fn select(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        if let Some(pos) = self.selected.iter().position(|&i| i == index) {
            self.selected.remove(pos);
        } else {
            self.selected.push(index);
        }
    }

    /// Advance the spinner animation frame.
    pub fn tick_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }

    // ─── View ─────────────────────────────────────────────────────────────

    /// Render the full Android pane (header + body).
    pub fn view<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
    ) -> Element<'a, Message> {
        let header = self.view_header(theme);
        let body = self.view_body(theme, on_navigate, on_select);
        column![header, body].width(Fill).height(Fill).into()
    }

    fn view_header<'a, Message: 'a + Clone>(&'a self, theme: ThemeColors) -> Element<'a, Message> {
        let header_text = match &self.state {
            AndroidPaneState::NoDevice => "Android Device — No device connected".to_string(),
            AndroidPaneState::Loading => {
                let dots = match self.spinner_frame % 4 {
                    0 => ".",
                    1 => "..",
                    2 => "...",
                    _ => "",
                };
                format!("Android Device — Loading{}", dots)
            }
            AndroidPaneState::Browsing => {
                let path = self.current_path.to_string_lossy();
                let display = if path.len() > 35 {
                    format!("…{}", &path[path.len() - 35..])
                } else {
                    path.into_owned()
                };
                format!("Android Device  {}", display)
            }
            AndroidPaneState::Error(msg) => format!("Android Device — Error: {}", msg),
        };

        container(text(header_text).size(12).color(theme.text_secondary))
            .width(Fill)
            .height(28.0)
            .padding([6, 12])
            .style(move |_t| container::Style {
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

    fn view_body<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
    ) -> Element<'a, Message> {
        match &self.state {
            AndroidPaneState::NoDevice => self.view_empty(
                theme,
                "No Android device connected",
                "Connect a device with USB debugging enabled",
            ),

            AndroidPaneState::Loading => self.view_loading(theme),

            AndroidPaneState::Error(msg) => {
                self.view_empty(theme, "Error", msg)
            }

            AndroidPaneState::Browsing => self.view_entries(theme, on_navigate, on_select),
        }
    }

    fn view_empty<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        title: &'a str,
        subtitle: &'a str,
    ) -> Element<'a, Message> {
        let content = column![
            text(title).size(13).color(theme.text_secondary),
            text(subtitle).size(11).color(theme.text_secondary),
        ]
        .spacing(6);

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

    fn view_loading<'a, Message: 'a + Clone>(&'a self, theme: ThemeColors) -> Element<'a, Message> {
        let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner = spinner_chars[(self.spinner_frame as usize) % spinner_chars.len()];

        let content = row![
            text(spinner).size(14).color(theme.accent),
            text("  Fetching directory listing…").size(12).color(theme.text_secondary),
        ]
        .align_y(iced::Alignment::Center);

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

    fn view_entries<'a, Message: 'a + Clone>(
        &'a self,
        theme: ThemeColors,
        on_navigate: impl Fn(PathBuf) -> Message + 'a,
        on_select: impl Fn(usize) -> Message + 'a,
    ) -> Element<'a, Message> {
        // Column header
        let col_header = container(
            row![
                text("Name").size(11).color(theme.text_secondary).width(Fill),
                text("Size").size(11).color(theme.text_secondary).width(80),
                text("Modified").size(11).color(theme.text_secondary).width(90),
            ]
            .padding([2, 12])
            .spacing(4),
        )
        .width(Fill)
        .style(move |_t| container::Style {
            background: Some(theme.background_secondary.into()),
            border: Border {
                color: theme.border,
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        });

        let mut rows: Vec<Element<Message>> = Vec::new();

        // ".." up-navigation (don't go above /sdcard)
        let is_at_root = self.current_path == PathBuf::from("/sdcard")
            || self.storage_roots.contains(&self.current_path);

        if !is_at_root {
            if let Some(parent) = self.current_path.parent() {
                let parent_path = parent.to_path_buf();
                let up_btn = button(
                    row![
                        text("📁").size(12),
                        text("..").size(12).color(theme.text),
                        iced::widget::Space::with_width(Fill),
                        text("").size(12).width(80),
                        text("").size(12).width(90),
                    ]
                    .spacing(6)
                    .padding([1, 0]),
                )
                .width(Fill)
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(on_navigate(parent_path));
                rows.push(up_btn.into());
            }
        }

        // Storage root quick-nav (shown at /sdcard level)
        if self.current_path == PathBuf::from("/sdcard") && self.storage_roots.len() > 1 {
            for root in &self.storage_roots {
                if root == &PathBuf::from("/sdcard") {
                    continue; // already here
                }
                let root_name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.to_string_lossy().into_owned());
                let root_path = root.clone();
                let btn = button(
                    row![
                        text("💾").size(12),
                        text(format!("{} (SD)", root_name))
                            .size(12)
                            .color(theme.accent),
                        iced::widget::Space::with_width(Fill),
                        text("--").size(11).color(theme.text_secondary).width(80),
                        text("").size(11).color(theme.text_secondary).width(90),
                    ]
                    .spacing(6)
                    .padding([1, 0]),
                )
                .width(Fill)
                .style(move |_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .on_press(on_navigate(root_path));
                rows.push(btn.into());
            }
        }

        // File/directory entries
        for (i, entry) in self.entries.iter().enumerate() {
            let is_selected = self.selected.contains(&i);
            let row_bg: Option<Color> = if is_selected {
                Some(theme.accent.scale_alpha(0.2))
            } else {
                None
            };

            let icon = if entry.is_symlink {
                "🔗"
            } else if entry.is_dir {
                "📁"
            } else {
                "📄"
            };

            let name_color = if entry.is_hidden {
                theme.text_secondary
            } else {
                theme.text
            };

            let entry_path = entry.path.clone();
            let entry_is_dir = entry.is_dir || entry.is_symlink;
            let on_nav = on_navigate(entry_path.clone());
            let on_sel = on_select(i);

            let row_content = row![
                text(icon).size(12),
                text(entry.name.clone())
                    .size(12)
                    .color(name_color)
                    .width(Fill),
                text(entry.size_display())
                    .size(11)
                    .color(theme.text_secondary)
                    .width(80),
                text(entry.modified.clone())
                    .size(11)
                    .color(theme.text_secondary)
                    .width(90),
            ]
            .spacing(6)
            .padding([1, 0]);

            let btn = button(row_content)
                .width(Fill)
                .style(move |_t, _s| button::Style {
                    background: row_bg.map(Into::into),
                    ..Default::default()
                })
                .on_press(if entry_is_dir { on_nav } else { on_sel });

            rows.push(btn.into());
        }

        let list = scrollable(column(rows).width(Fill).padding([0, 4]))
            .width(Fill)
            .height(Fill);

        container(
            column![col_header, list]
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(move |_t| container::Style {
            background: Some(theme.background.into()),
            ..Default::default()
        })
        .into()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::AndroidEntry;
    use std::path::PathBuf;

    fn make_entry(name: &str, is_dir: bool) -> AndroidEntry {
        AndroidEntry {
            name: name.to_string(),
            path: PathBuf::from("/sdcard").join(name),
            size: if is_dir { 0 } else { 1024 },
            modified: "2024-01-15".to_string(),
            is_dir,
            is_symlink: false,
            is_hidden: name.starts_with('.'),
        }
    }

    #[test]
    fn default_state_is_no_device() {
        let pane = AndroidPane::default();
        assert_eq!(pane.state, AndroidPaneState::NoDevice);
        assert!(pane.entries.is_empty());
    }

    #[test]
    fn on_device_connected_transitions_to_loading() {
        let mut pane = AndroidPane::default();
        pane.on_device_connected();
        assert_eq!(pane.state, AndroidPaneState::Loading);
        assert_eq!(pane.current_path, PathBuf::from("/sdcard"));
    }

    #[test]
    fn on_device_disconnected_returns_to_no_device() {
        let mut pane = AndroidPane::default();
        pane.on_device_connected();
        pane.on_device_disconnected();
        assert_eq!(pane.state, AndroidPaneState::NoDevice);
        assert!(pane.entries.is_empty());
        assert!(pane.storage_roots.is_empty());
    }

    #[test]
    fn begin_navigate_transitions_to_loading() {
        let mut pane = AndroidPane::default();
        pane.begin_navigate(PathBuf::from("/sdcard/DCIM"));
        assert_eq!(pane.state, AndroidPaneState::Loading);
        assert_eq!(pane.current_path, PathBuf::from("/sdcard/DCIM"));
        assert!(pane.selected.is_empty());
    }

    #[test]
    fn on_entries_loaded_transitions_to_browsing() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());

        let entries = vec![make_entry("DCIM", true), make_entry("photo.jpg", false)];
        let roots = vec![PathBuf::from("/sdcard")];
        pane.on_entries_loaded(path, entries, roots);

        assert_eq!(pane.state, AndroidPaneState::Browsing);
        assert_eq!(pane.entries.len(), 2);
        // dirs-first sort: DCIM before photo.jpg
        assert!(pane.entries[0].is_dir);
    }

    #[test]
    fn on_entries_loaded_stale_response_ignored() {
        let mut pane = AndroidPane::default();
        pane.begin_navigate(PathBuf::from("/sdcard/DCIM")); // navigated here

        // Response arrives for /sdcard (old path)
        let stale_entries = vec![make_entry("old.jpg", false)];
        pane.on_entries_loaded(PathBuf::from("/sdcard"), stale_entries, vec![]);

        // State should still be Loading (not Browsing), entries unchanged
        assert_eq!(pane.state, AndroidPaneState::Loading);
        assert!(pane.entries.is_empty());
    }

    #[test]
    fn on_error_transitions_to_error() {
        let mut pane = AndroidPane::default();
        pane.begin_navigate(PathBuf::from("/sdcard/private"));
        pane.on_error("Permission denied".to_string());
        assert!(matches!(pane.state, AndroidPaneState::Error(_)));
    }

    #[test]
    fn select_toggles_entry() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());
        pane.on_entries_loaded(
            path,
            vec![make_entry("DCIM", true), make_entry("Music", true)],
            vec![],
        );

        // First click — selects
        pane.select(1);
        assert!(pane.selected.contains(&1));

        // Second click — deselects
        pane.select(1);
        assert!(!pane.selected.contains(&1));
    }

    #[test]
    fn select_multi_adds_both() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());
        pane.on_entries_loaded(
            path,
            vec![make_entry("DCIM", true), make_entry("Music", true)],
            vec![],
        );

        pane.select(0);
        pane.select(1);
        assert!(pane.selected.contains(&0));
        assert!(pane.selected.contains(&1));
    }

    #[test]
    fn select_out_of_bounds_ignored() {
        let mut pane = AndroidPane::default();
        pane.select(99);
        assert!(pane.selected.is_empty());
    }

    #[test]
    fn tick_spinner_advances_and_wraps() {
        let mut pane = AndroidPane::default();
        assert_eq!(pane.spinner_frame, 0);
        for _ in 0..255 {
            pane.tick_spinner();
        }
        // After 255 ticks, wrapping_add(1) gives 0 on the next tick
        pane.tick_spinner();
        assert_eq!(pane.spinner_frame, 0);
    }

    #[test]
    fn entries_sorted_dirs_first() {
        let mut pane = AndroidPane::default();
        let path = PathBuf::from("/sdcard");
        pane.begin_navigate(path.clone());
        let entries = vec![
            make_entry("zebra.txt", false),
            make_entry("alpha_dir", true),
            make_entry("beta_dir", true),
        ];
        pane.on_entries_loaded(path, entries, vec![]);
        assert!(pane.entries[0].is_dir);
        assert!(pane.entries[1].is_dir);
        assert!(!pane.entries[2].is_dir);
        assert_eq!(pane.entries[0].name, "alpha_dir");
    }
}
