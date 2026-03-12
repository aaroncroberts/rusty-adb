//! Dark theme color system for rusty-adb
//!
//! Defines a consistent color palette for the application following
//! a dark, sleek, modern aesthetic — ported from rusty-app.

use iced::Color;

/// Main color palette for the dark theme
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeColors {
    /// Main background color (#23272e)
    pub background: Color,
    /// Secondary background for panels (#2d323b)
    pub background_secondary: Color,
    /// Border color (#353b45)
    pub border: Color,
    /// Accent color for highlights (#5fb3a6)
    pub accent: Color,
    /// Primary text color (#f5f6fa)
    pub text: Color,
    /// Secondary text color, dimmed (#9ca0a8)
    pub text_secondary: Color,
    /// Success indicator (#50c878)
    pub success: Color,
    /// Warning indicator (#f39c12)
    pub warning: Color,
    /// Error indicator (#e74c3c)
    pub error: Color,
}

impl ThemeColors {
    /// Create the default dark theme palette
    pub const fn dark() -> Self {
        Self {
            background: Color::from_rgb(
                0x23 as f32 / 255.0,
                0x27 as f32 / 255.0,
                0x2e as f32 / 255.0,
            ),
            background_secondary: Color::from_rgb(
                0x2d as f32 / 255.0,
                0x32 as f32 / 255.0,
                0x3b as f32 / 255.0,
            ),
            border: Color::from_rgb(
                0x35 as f32 / 255.0,
                0x3b as f32 / 255.0,
                0x45 as f32 / 255.0,
            ),
            accent: Color::from_rgb(
                0x5f as f32 / 255.0,
                0xb3 as f32 / 255.0,
                0xa6 as f32 / 255.0,
            ),
            text: Color::from_rgb(
                0xf5 as f32 / 255.0,
                0xf6 as f32 / 255.0,
                0xfa as f32 / 255.0,
            ),
            text_secondary: Color::from_rgb(
                0x9c as f32 / 255.0,
                0xa0 as f32 / 255.0,
                0xa8 as f32 / 255.0,
            ),
            success: Color::from_rgb(
                0x50 as f32 / 255.0,
                0xc8 as f32 / 255.0,
                0x78 as f32 / 255.0,
            ),
            warning: Color::from_rgb(
                0xf3 as f32 / 255.0,
                0x9c as f32 / 255.0,
                0x12 as f32 / 255.0,
            ),
            error: Color::from_rgb(
                0xe7 as f32 / 255.0,
                0x4c as f32 / 255.0,
                0x3c as f32 / 255.0,
            ),
        }
    }
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::dark()
    }
}
