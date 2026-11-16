//! `~/.config/minka-settings.json` + MinkaSyu state, and the Duo/general
//! output rules from `services/ShellLayout.qml`.
//!
//! The QML watches both files with `FileView watchChanges`; here the service
//! ticker polls their mtimes (AntiWebUI has no public file-watch API) and
//! reparses on change, matching the QML's reset-on-missing/malformed rules.

use anti_webui::shell::prelude::*;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `"duo" | "general" | "auto"` (anything unrecognised is `"auto"`).
    pub layout: String,
    /// Absolute wallpaper path, `""` for none.
    pub wallpaper: String,
    /// Only an explicit `false` disables the workspace pills.
    pub workspaces_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            layout: "auto".to_owned(),
            wallpaper: String::new(),
            workspaces_enabled: true,
        }
    }
}

impl Settings {
    pub fn parse(text: &str) -> Self {
        let Ok(data) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let layout = match data
            .get("shell")
            .and_then(|s| s.get("layout"))
            .and_then(Value::as_str)
        {
            Some("duo") => "duo",
            Some("general") => "general",
            _ => "auto",
        }
        .to_owned();
        let wallpaper = data
            .get("shell")
            .and_then(|s| s.get("wallpaper"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let workspaces_enabled = !matches!(
            data.get("workspaces")
                .and_then(|w| w.get("enabled"))
                .and_then(Value::as_bool),
            Some(false)
        );
        Self {
            layout,
            wallpaper,
            workspaces_enabled,
        }
    }

    pub fn load() -> Self {
        settings_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }
}

pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(|home| PathBuf::from(home).join(".config/minka-settings.json"))
}

/// ScreenPad is matched by shape, never by connector name (UX482: 1920x515).
pub fn is_screenpad(output: &Output) -> bool {
    output.logical_size.height <= 600 && output.logical_size.width >= 1600
}

pub fn screenpad_present(all: &[Output]) -> bool {
    all.iter().any(is_screenpad)
}

pub fn duo_mode(settings: &Settings, all: &[Output]) -> bool {
    match settings.layout.as_str() {
        "duo" => true,
        "general" => false,
        _ => screenpad_present(all),
    }
}

/// Which outputs carry persistent shell surfaces. Forced-duo with no pad
/// falls back to every output so the session is never shell-less.
pub fn show_bar_on(settings: &Settings, all: &[Output], output: &Output) -> bool {
    if !duo_mode(settings, all) {
        return true;
    }
    if screenpad_present(all) {
        is_screenpad(output)
    } else {
        true
    }
}

/// Whether a MinkaSyu update row is one a plain -Syu applies. Rows from
/// cores before held/superseded rows existed have neither key.
fn pending(u: &Value) -> bool {
    u.get("held").and_then(Value::as_bool) != Some(true)
        && u.get("superseded_by").and_then(Value::as_array).is_none_or(|a| a.is_empty())
}

/// MinkaSyu's state file, watched by `services/SyuState.qml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyuState {
    pub count: u64,
    pub repo_changes: u64,
    pub checked_at: i64,
}

impl SyuState {
    pub fn parse(text: &str) -> Self {
        let Ok(data) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        Self {
            // Pending rows only: MinkaSyu also lists updates pacman.conf
            // holds back ("held") and packages a ticked replacement
            // removes ("superseded_by"), which a plain -Syu does not apply.
            count: data
                .get("updates")
                .and_then(Value::as_array)
                .map_or(0, |a| a.iter().filter(|u| pending(u)).count() as u64),
            repo_changes: data
                .get("repo_change_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            checked_at: data.get("checked_at").and_then(Value::as_i64).unwrap_or(0),
        }
    }

    pub fn load() -> Self {
        syu_state_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }
}

pub fn syu_state_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MINKA_SYU_STATE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("minka-syu/state.json"))
}

/// MinkaSyu UI directory for the updates-pill launch command.
/// The MinkaSyu core, which also opens its window (`minka-syu ui`).
pub fn syu_bin() -> String {
    std::env::var("MINKA_SYU_BIN")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "minka-syu".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_parse_rules() {
        let s = Settings::parse(r#"{"shell":{"layout":"duo","wallpaper":"/w.png"}}"#);
        assert_eq!(s.layout, "duo");
        assert_eq!(s.wallpaper, "/w.png");
        assert!(s.workspaces_enabled);
        let s = Settings::parse(r#"{"shell":{"layout":"bogus"},"workspaces":{"enabled":false}}"#);
        assert_eq!(s.layout, "auto");
        assert!(!s.workspaces_enabled);
        assert_eq!(Settings::parse("not json"), Settings::default());
    }

    #[test]
    fn syu_parse_rules() {
        let s = SyuState::parse(r#"{"updates":[1,2],"repo_change_count":3,"checked_at":9}"#);
        assert_eq!(s.count, 2);
        assert_eq!(s.repo_changes, 3);
        assert_eq!(s.checked_at, 9);
        assert_eq!(SyuState::parse("{}"), SyuState::default());
        let held = SyuState::parse(
            r#"{"updates":[{"name":"a"},{"name":"b","held":true},{"name":"c","superseded_by":["d"]},{"name":"d","superseded_by":[]}]}"#,
        );
        assert_eq!(held.count, 2);
    }
}
