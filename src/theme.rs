//! Proustite theme tokens and small text helpers.
//!
//! The values mirror the shared `Proustite` QML singleton and MinkaShell's
//! `services/Theme.qml` facade. They are duplicated here as constants because
//! the Rust application cannot consume the QML singleton; keep them in sync
//! with `../../Proustite/Proustite.qml`.

use anti_webui::prelude::*;

pub const GROUND: u32 = 0x0a0709;
pub const SURFACE: u32 = 0x161013;
pub const SURFACE_RAISED: u32 = 0x1e161a;
pub const LINE: u32 = 0x2e2228;
pub const TEXT: u32 = 0xece5e7;
pub const TEXT_MUTED: u32 = 0xa3959b;
pub const TEXT_FAINT: u32 = 0x6e6167;
pub const RED: u32 = 0xff0000;
pub const RED_DIM: u32 = 0x8f1e2d;
pub const PURPLE: u32 = 0xa488c9;
pub const PURPLE_DIM: u32 = 0x4c3a63;
pub const WARN_AMBER: u32 = 0xe0a026;

/// `ground` at 0.92 alpha: bar and menu card background.
pub const BAR_BG: u32 = 0x0a0709;
pub const BAR_BG_ALPHA: f32 = 0.92;
pub const BAR_HEIGHT: f32 = 32.0;
pub const DOCK_HEIGHT: f32 = 44.0;

/// MinkaDE's UI font, then what it was before. The toolkit skips a family
/// that is not installed without saying so, and Roboto Flex is in no
/// official Arch repo (AUR: ttf-roboto-flex), so Noto Sans stays in the list.
/// Proustite.qml names Roboto Flex alone, since QML takes one family.
pub const FONT_FAMILY: &str = "Roboto Flex, Noto Sans";
pub const MONO_FAMILY: &str = "monospace";
pub const FONT_SIZE: f32 = 13.0;

pub fn color(hex: u32) -> Color {
    Color::from_rgb(hex)
}

pub fn color_alpha(hex: u32, alpha: f32) -> Color {
    Color::from_rgb(hex).with_alpha(alpha.clamp(0.0, 1.0))
}

pub fn bar_bg() -> Color {
    color_alpha(BAR_BG, BAR_BG_ALPHA)
}

pub fn ui_text(source: impl IntoTextSource, size: f32, hex: u32) -> Text {
    text(source)
        .font_family(FONT_FAMILY)
        .text_size(size)
        .text_color(color(hex))
}

/// Elide-right to fit `max_px` at `font_size`. AntiWebUI has no text
/// elision and no no-wrap style, and the QML elides titles rather than
/// wrapping them; 0.62em/char is deliberately conservative so the elided
/// string never wraps either.
pub fn elide(text: &str, max_px: f32, font_size: f32) -> String {
    let budget = (max_px / (font_size * 0.62)) as usize;
    if text.chars().count() <= budget.max(1) {
        return text.to_owned();
    }
    let keep = budget.saturating_sub(1).max(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

pub fn mono_text(source: impl IntoTextSource, size: f32, hex: u32) -> Text {
    text(source)
        .font_family(MONO_FAMILY)
        .text_size(size)
        .text_color(color(hex))
}
