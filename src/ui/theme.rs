//! Design tokens — single source of truth for all colors.
#![allow(dead_code)]

use ratatui::style::Color;

// Background
pub const BG_BASE: Color = Color::Rgb(26, 24, 20);
pub const BG_SURFACE: Color = Color::Rgb(34, 32, 28);
pub const BG_SELECTED: Color = Color::Rgb(42, 38, 33);

// Borders
pub const BORDER_MUTED: Color = Color::Rgb(58, 54, 49);

// Text
pub const TEXT_PRIMARY: Color = Color::Rgb(232, 227, 216);
pub const TEXT_SECONDARY: Color = Color::Rgb(184, 179, 168);
pub const TEXT_MUTED: Color = Color::Rgb(138, 132, 122);
pub const TEXT_DIM: Color = Color::Rgb(90, 85, 76);

// Accents
pub const ACCENT_GOLD: Color = Color::Rgb(212, 165, 116);
pub const ACCENT_SAGE: Color = Color::Rgb(139, 168, 136);
pub const ACCENT_TERRA: Color = Color::Rgb(194, 122, 111);
