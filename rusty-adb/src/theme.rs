//! Retro terminal color system for rusty-adb
//!
//! Phosphor-green-on-near-black palette that evokes classic CRT terminals
//! (think Midnight Commander or a green-screen VT100).

use iced::widget::{button, container};
use iced::{Border, Color};

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

impl ThemeColors {
    // ── Style helpers — return closures for use with .style() ─────────────────

    /// Transparent/ghost button — no background, no border. For text-only and icon buttons.
    pub fn transparent_button(self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        move |_, _| button::Style {
            background: None,
            ..Default::default()
        }
    }

    /// Accent-filled button — prominent primary action (install, save).
    ///
    /// Uses a deep forest-green fill so white label text is clearly legible
    /// while still standing out from the near-black `secondary_button`.
    pub fn accent_button(self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        move |_, status| {
            // Deep emerald: visible as "primary action" without neon glare.
            let base = Color::from_rgb(
                0x0d as f32 / 255.0,
                0x6e as f32 / 255.0,
                0x44 as f32 / 255.0,
            );
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    // Slightly lighter on hover/press for feedback
                    Color::from_rgb(
                        0x12 as f32 / 255.0,
                        0x8a as f32 / 255.0,
                        0x56 as f32 / 255.0,
                    )
                }
                _ => base,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: Color::WHITE,
                border: Border {
                    color: self.accent.scale_alpha(0.4),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        }
    }

    /// Secondary button — `background_secondary` fill with a 1px border. Standard action.
    pub fn secondary_button(self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        move |_, _| button::Style {
            background: Some(self.background_secondary.into()),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    }

    /// Error/destructive button — red fill for dangerous actions (delete).
    pub fn error_button(self) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
        move |_, _| button::Style {
            background: Some(self.error.into()),
            border: Border {
                radius: 0.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Secondary panel — `background_secondary` fill with a 1px border. Most common panel style.
    pub fn secondary_panel(self) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(self.background_secondary.into()),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    }

    /// Primary panel — `background` fill with a 1px border. For modal cards and main content areas.
    pub fn primary_panel(self) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(self.background.into()),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    }

    /// Error notification banner — subtle red tint with red border.
    pub fn error_banner(self) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(self.error.scale_alpha(0.12).into()),
            border: Border {
                color: self.error.scale_alpha(0.4),
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Warning/delete-confirm banner — subtle amber tint.
    pub fn warning_banner(self) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(self.warning.scale_alpha(0.15).into()),
            border: Border {
                color: self.warning.scale_alpha(0.5),
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Success/toast banner — subtle green tint.
    pub fn success_banner(self) -> impl Fn(&iced::Theme) -> container::Style {
        move |_| container::Style {
            background: Some(self.success.scale_alpha(0.15).into()),
            border: Border {
                color: self.success.scale_alpha(0.5),
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::retro()
    }
}
