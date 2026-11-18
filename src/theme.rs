//! Proustite theme tokens, the Material 3 Expressive color scheme made from
//! them, and small text helpers.
//!
//! The values mirror the shared `Proustite` QML singleton and MinkaShell's
//! `services/Theme.qml` facade. They are duplicated here as constants because
//! the Rust application cannot consume the QML singleton; keep them in sync
//! with `../../Proustite/Proustite.qml`.

use anti_webui::prelude::*;
use anti_webui_md3e::{ColorScheme, Density, set_color_scheme, set_density};

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
pub const GAUGE_DIM: u32 = 0x3a1219;
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

/// What text selected in a field is marked with.
pub fn selection_bg() -> Color {
    color_alpha(RED_DIM, 0.5)
}

/// The thumb of a scroller.
pub fn scrollbar_fg() -> Color {
    color_alpha(TEXT_FAINT, 0.6)
}

/// Proustite's tokens in Material's color roles: what the md3e components
/// draw with. The roles Proustite has no token for (the error ones and the
/// scrim) are the ones Material grows from Proustite's red.
pub fn scheme() -> ColorScheme {
    ColorScheme {
        primary: color(RED),
        on_primary: color(GROUND),
        primary_container: color(RED_DIM),
        on_primary_container: color(TEXT),
        secondary: color(PURPLE),
        on_secondary: color(GROUND),
        secondary_container: color(PURPLE_DIM),
        on_secondary_container: color(TEXT),
        tertiary: color(WARN_AMBER),
        on_tertiary: color(GROUND),
        tertiary_container: color(GAUGE_DIM),
        on_tertiary_container: color(TEXT),
        surface: color(GROUND),
        surface_dim: color(GROUND),
        surface_bright: color(LINE),
        surface_variant: color(SURFACE_RAISED),
        on_surface: color(TEXT),
        on_surface_variant: color(TEXT_MUTED),
        inverse_surface: color(TEXT),
        inverse_on_surface: color(GROUND),
        inverse_primary: color(RED_DIM),
        surface_container_lowest: color(GROUND),
        surface_container_low: color(SURFACE),
        surface_container: color(SURFACE),
        surface_container_high: color(SURFACE_RAISED),
        surface_container_highest: color(LINE),
        outline: color(TEXT_FAINT),
        outline_variant: color(LINE),
        ..ColorScheme::from_seed(color(RED), true)
    }
}

/// How everything the shell builds from here on draws: the md3e components
/// in their compact sizes and the Proustite scheme, and the toolkit's own
/// widgets (the start menu's search field, tray menus), which default to a
/// light theme, in Proustite's colors. Call before the first view.
pub fn install() {
    set_density(Density::Compact);
    set_color_scheme(scheme());
    set_theme(Theme {
        surface: color(SURFACE),
        surface_muted: color(SURFACE),
        overlay: color(SURFACE_RAISED),
        hover: color(SURFACE_RAISED),
        pressed: color(LINE),
        selected: color(PURPLE_DIM),
        text: color(TEXT),
        text_muted: color(TEXT_MUTED),
        text_disabled: color(TEXT_FAINT),
        on_accent: color(TEXT),
        border: color(LINE),
        border_strong: color(TEXT_FAINT),
        accent: color(PURPLE_DIM),
        accent_hover: color(PURPLE),
        accent_pressed: color(PURPLE_DIM),
        accent_text: color(PURPLE),
        accent_muted: color(GAUGE_DIM),
        warning: color(WARN_AMBER),
        danger: color(RED),
        tooltip: color(SURFACE_RAISED),
        on_tooltip: color(TEXT),
        focus_ring: color_alpha(PURPLE, 0.7),
        field_focus_ring: color_alpha(PURPLE, 0.35),
        ..Theme::dark()
    });
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
