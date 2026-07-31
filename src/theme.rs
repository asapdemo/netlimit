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

pub const ACCENT: Color = Color::Rgb(88, 166, 255);
pub const DOWNLOAD: Color = Color::Rgb(88, 166, 255);
pub const UPLOAD: Color = Color::Rgb(63, 185, 80);
pub const LOSS: Color = Color::Rgb(210, 153, 34);
pub const SUCCESS: Color = Color::Rgb(63, 185, 80);
pub const ERROR: Color = Color::Rgb(248, 81, 73);
pub const WARN: Color = Color::Rgb(210, 153, 34);
