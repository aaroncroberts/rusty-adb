//! Retro terminal color system for rusty-adb
//!
//! Phosphor-green-on-near-black palette that evokes classic CRT terminals
//! (think Midnight Commander or a green-screen VT100).

use iced::Color;

/// Main color palette — retro terminal aesthetic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeColors {
    /// Near-black CRT background (#080c08)
    pub background: Color,
    /// Slightly lighter panel background (#0f160f)
    pub background_secondary: Color,
    /// Dark phosphor-green border (#1e3a1e)
    pub border: Color,
    /// Bright phosphor-green accent (#33dd33)
    pub accent: Color,
    /// Soft green primary text (#c0ecc0)
    pub text: Color,
    /// Dimmer green secondary text (#5a8a5a)
    pub text_secondary: Color,
    /// Bright mint success indicator (#00ff88)
    pub success: Color,
    /// Amber warning indicator (#ccaa00)
    pub warning: Color,
    /// Muted red error indicator (#dd2222)
    pub error: Color,
}

impl ThemeColors {
    /// Retro terminal palette — phosphor green on near-black.
    pub const fn dark() -> Self {
        Self::retro()
    }

    /// Retro terminal palette — phosphor green on near-black CRT.
    pub const fn retro() -> Self {
        Self {
            // Near-black CRT background
            background: Color::from_rgb(
                0x08 as f32 / 255.0,
                0x0c as f32 / 255.0,
                0x08 as f32 / 255.0,
            ),
            // Panel / secondary background
            background_secondary: Color::from_rgb(
                0x0f as f32 / 255.0,
                0x16 as f32 / 255.0,
                0x0f as f32 / 255.0,
            ),
            // Dark green border
            border: Color::from_rgb(
                0x1e as f32 / 255.0,
                0x3a as f32 / 255.0,
                0x1e as f32 / 255.0,
            ),
            // Bright phosphor green — accent highlights & icons
            accent: Color::from_rgb(
                0x33 as f32 / 255.0,
                0xdd as f32 / 255.0,
                0x33 as f32 / 255.0,
            ),
            // Soft green — primary readable text
            text: Color::from_rgb(
                0xc0 as f32 / 255.0,
                0xec as f32 / 255.0,
                0xc0 as f32 / 255.0,
            ),
            // Dimmer green — secondary / metadata text
            text_secondary: Color::from_rgb(
                0x5a as f32 / 255.0,
                0x8a as f32 / 255.0,
                0x5a as f32 / 255.0,
            ),
            // Bright mint — success
            success: Color::from_rgb(
                0x00 as f32 / 255.0,
                0xff as f32 / 255.0,
                0x88 as f32 / 255.0,
            ),
            // Amber — warning (CRT yellow-orange)
            warning: Color::from_rgb(
                0xcc as f32 / 255.0,
                0xaa as f32 / 255.0,
                0x00 as f32 / 255.0,
            ),
            // Muted red — error
            error: Color::from_rgb(
                0xdd as f32 / 255.0,
                0x22 as f32 / 255.0,
                0x22 as f32 / 255.0,
            ),
        }
    }
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::retro()
    }
}
