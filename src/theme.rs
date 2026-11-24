//! Proustite (the `proustite` crate, generated from Proustite.qml) and the
//! shell's own tokens, as `services/Theme.qml` is a facade over the QML
//! singleton.

use anti_webui::prelude::Color;
use anti_webui_md3e::{Density, set_density};
pub use proustite::*;

/// `ground` at 0.92 alpha: bar and menu card background.
pub const BAR_BG: u32 = GROUND;
pub const BAR_BG_ALPHA: f32 = 0.92;
pub const BAR_HEIGHT: f32 = 32.0;
pub const DOCK_HEIGHT: f32 = 44.0;

pub fn bar_bg() -> Color {
    color_alpha(BAR_BG, BAR_BG_ALPHA)
}

/// How everything the shell builds from here on draws: the md3e components
/// in their compact sizes, and both widget sets in Proustite. Call before the
/// first view.
pub fn install() {
    set_density(Density::Compact);
    proustite::install();
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
