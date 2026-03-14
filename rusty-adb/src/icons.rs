//! Icon helpers — Nerd Font (Codicons subset) for use with iced text widgets.
#![allow(dead_code)]
//!
//! Load the font at app startup by chaining `.font(FONT_BYTES)` onto the
//! `iced::application()` builder (see `main.rs`).
//!
//! Usage in view code:
//! ```ignore
//! use crate::icons;
//! text(icons::folder()).font(icons::FONT).size(14).color(t.accent)
//! ```

use nerd_font::categories::Cod;

/// Raw font bytes — pass to `.font(icons::FONT_BYTES)` on the iced application builder.
pub const FONT_BYTES: &[u8] = nerd_font::NerdFont::FONT_BYTES;

/// The `iced::Font` definition for the Nerd Font family.
///
/// fontdb registers fonts by **Typographic Family (name ID 16)**, which is
/// `"JetBrainsMono Nerd Font"` for this TTF.  The `nerd_font::IcedExt` trait
/// uses **Family (ID 1)** = `"JetBrainsMono NF Medium"` — a mismatch.
/// We build the `Font` struct directly with the correct family name and weight.
pub fn font() -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name("JetBrainsMono Nerd Font"),
        weight: iced::font::Weight::Medium,
        ..Default::default()
    }
}

// ── File system icons ─────────────────────────────────────────────────────────

/// Closed folder glyph.
pub fn folder() -> String {
    Cod::Folder.to_string()
}

/// Open / navigated-into folder glyph.
pub fn folder_open() -> String {
    Cod::FolderOpened.to_string()
}

/// Regular file glyph.
pub fn file() -> String {
    Cod::File.to_string()
}

/// Symbolic-link file glyph.
pub fn symlink() -> String {
    Cod::FileSymlinkFile.to_string()
}

// ── Pane layout icons ─────────────────────────────────────────────────────────

/// Expand-pane glyph (split → one pane fills screen).
pub fn expand() -> String {
    Cod::ScreenFull.to_string()
}

/// Collapse-pane glyph (expanded → restore split).
pub fn collapse() -> String {
    Cod::ScreenNormal.to_string()
}

// ── Action / toolbar icons ────────────────────────────────────────────────────

/// Gear / settings glyph.
pub fn settings() -> String {
    Cod::SettingsGear.to_string()
}

/// Refresh / reload glyph.
pub fn refresh() -> String {
    Cod::Refresh.to_string()
}

/// Delete / trash glyph.
pub fn trash() -> String {
    Cod::Trash.to_string()
}

/// Upload-to-device glyph.
pub fn upload() -> String {
    Cod::CloudUpload.to_string()
}

/// Download-from-device glyph.
pub fn download() -> String {
    Cod::CloudDownload.to_string()
}

/// New-folder glyph.
pub fn new_folder() -> String {
    Cod::NewFolder.to_string()
}

/// Close / X glyph.
pub fn close() -> String {
    Cod::Close.to_string()
}

/// Chevron pointing right (sort ascending indicator).
pub fn chevron_right() -> String {
    Cod::ChevronRight.to_string()
}

/// Chevron pointing down (sort descending indicator).
pub fn chevron_down() -> String {
    Cod::ChevronDown.to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::const_is_empty)]
    fn font_bytes_non_empty() {
        assert!(!FONT_BYTES.is_empty());
    }

    #[test]
    fn all_icon_strings_non_empty() {
        assert!(!folder().is_empty());
        assert!(!folder_open().is_empty());
        assert!(!file().is_empty());
        assert!(!symlink().is_empty());
        assert!(!expand().is_empty());
        assert!(!collapse().is_empty());
        assert!(!settings().is_empty());
        assert!(!refresh().is_empty());
        assert!(!trash().is_empty());
        assert!(!upload().is_empty());
        assert!(!download().is_empty());
        assert!(!new_folder().is_empty());
        assert!(!close().is_empty());
    }
}
