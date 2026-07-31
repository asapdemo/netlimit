//! btop-inspired dark theme tokens.

use ratatui::style::Color;

/// Background / surfaces (GitHub-dark + btop density).
pub const BG: Color = Color::Rgb(13, 17, 23);
pub const SURFACE: Color = Color::Rgb(22, 27, 34);
pub const SURFACE_ALT: Color = Color::Rgb(28, 35, 51);
pub const BORDER: Color = Color::Rgb(48, 54, 61);

pub const TEXT: Color = Color::Rgb(201, 209, 217);
pub const TEXT_DIM: Color = Color::Rgb(139, 148, 158);
pub const TEXT_MUTED: Color = Color::Rgb(110, 118, 129);
pub const TEXT_INVERSE: Color = Color::Rgb(255, 255, 255);

pub const ACCENT: Color = Color::Rgb(88, 166, 255);
pub const DOWNLOAD: Color = Color::Rgb(88, 166, 255);
pub const UPLOAD: Color = Color::Rgb(63, 185, 80);
pub const LOSS: Color = Color::Rgb(210, 153, 34);
pub const SUCCESS: Color = Color::Rgb(63, 185, 80);
pub const ERROR: Color = Color::Rgb(248, 81, 73);
pub const WARN: Color = Color::Rgb(210, 153, 34);

/// Filled button backgrounds (slightly softer than pure accent).
pub const BTN_PRIMARY_BG: Color = Color::Rgb(35, 134, 54);
pub const BTN_PRIMARY_BORDER: Color = Color::Rgb(46, 160, 67);
pub const BTN_DANGER_BG: Color = Color::Rgb(48, 24, 28);
pub const BTN_DANGER_BORDER: Color = Color::Rgb(248, 81, 73);
pub const BTN_ACCENT_BG: Color = Color::Rgb(24, 40, 64);
pub const BTN_ACCENT_BORDER: Color = Color::Rgb(88, 166, 255);
pub const BTN_GHOST_BG: Color = Color::Rgb(33, 38, 45);
pub const BTN_GHOST_BORDER: Color = Color::Rgb(72, 79, 88);
pub const BTN_STEP_BG: Color = Color::Rgb(33, 38, 45);
pub const BTN_CHIP_BG: Color = Color::Rgb(22, 27, 34);
pub const BTN_CHIP_ACTIVE_BG: Color = Color::Rgb(28, 40, 58);
