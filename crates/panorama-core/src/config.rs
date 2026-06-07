//! XDG-compliant persistence for runtime config and display state.
//!
//! Two files, both JSON:
//! - `$XDG_CONFIG_HOME/tryx-panorama-mgr/config.json` — user preferences
//! - `$XDG_STATE_HOME/tryx-panorama-mgr/display.json` — last-applied display state
//!
//! `XDG_CONFIG_HOME` and `XDG_STATE_HOME` fall back to `$HOME/.config` and
//! `$HOME/.local/state` respectively, matching the XDG Base Directory spec.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const APP_NAME: &str = "tryx-panorama-mgr";
const CONFIG_FILENAME: &str = "config.json";
const STATE_FILENAME: &str = "display.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: String,
    pub brightness: i32,
    pub keepalive_interval: i32,
    #[serde(rename = "fanLcdPercent")]
    pub fan_lcd_percent: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: String::new(),
            // Default below max to reduce display burn-in risk.
            brightness: 75,
            // The cooler stops responding to commands after a longer idle
            // gap, so the daemon must send traffic at least this often
            // (seconds). 10 s is too slow — the device goes silent.
            keepalive_interval: 2,
            fan_lcd_percent: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayState {
    pub media: Vec<String>,
    /// Pane-2 media when `screen_mode == "Screen Splitting"`. Empty in
    /// full-screen mode and when the user has explicitly cleared pane 2.
    pub media2: Vec<String>,
    /// On-screen system-metric overlay labels (max 3), e.g. "CPU Temperature".
    pub sysinfo_display: Vec<String>,
    /// Pane-2 metric overlay labels (max 3). Used only in split mode.
    pub sysinfo_display2: Vec<String>,
    pub badges: Vec<String>,
    pub badges2: Vec<String>,
    pub ratio: String,
    pub screen_mode: String,
    pub play_mode: String,
    pub brightness: i32,
    /// Metrics overlay appearance: text color (`#RRGGBB`), horizontal
    /// alignment (`Left`/`Center`/`Right`), vertical position
    /// (`Top`/`Center`/`Bottom`).
    pub metrics_color: String,
    pub metrics_align: String,
    pub metrics_position: String,
    /// Firmware display filter effect. Empty disables the effect; known values
    /// include `Smoke` and `Rain`.
    pub display_filter: String,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            media: Vec::new(),
            media2: Vec::new(),
            sysinfo_display: Vec::new(),
            sysinfo_display2: Vec::new(),
            badges: Vec::new(),
            badges2: Vec::new(),
            ratio: "2:1".to_string(),
            screen_mode: "Full Screen".to_string(),
            play_mode: "Single".to_string(),
            brightness: 75,
            metrics_color: "#FFFFFF".to_string(),
            metrics_align: "Left".to_string(),
            metrics_position: "Top".to_string(),
            display_filter: String::new(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    xdg_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
        ".config",
    )
}

pub fn state_dir() -> PathBuf {
    xdg_path(
        std::env::var("XDG_STATE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
        ".local/state",
    )
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILENAME)
}

pub fn state_path() -> PathBuf {
    state_dir().join(STATE_FILENAME)
}

/// Load config from the default XDG path.
///
/// Returns `Some(Config::default())` when the file is absent — first-run
/// callers always get usable defaults. Returns `None` only when the file
/// exists but is unreadable, empty, or contains malformed JSON.
pub fn load_config() -> Option<Config> {
    load_config_from(&config_path())
}

pub fn save_config(config: &Config) -> bool {
    save_config_to(config, &config_path())
}

/// Load display state from the default XDG path.
///
/// Returns `None` when no state file exists — callers can distinguish
/// "nothing saved yet" from "saved state was empty".
pub fn load_state() -> Option<DisplayState> {
    load_state_from(&state_path())
}

pub fn save_state(state: &DisplayState) -> bool {
    save_state_to(state, &state_path())
}

pub fn load_config_from(path: &Path) -> Option<Config> {
    if !path.exists() {
        return Some(Config::default());
    }
    let raw = std::fs::read_to_string(path).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&raw).ok()
}

pub fn save_config_to(config: &Config, path: &Path) -> bool {
    save_json(config, path)
}

pub fn load_state_from(path: &Path) -> Option<DisplayState> {
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&raw).ok()
}

pub fn save_state_to(state: &DisplayState, path: &Path) -> bool {
    save_json(state, path)
}

fn save_json<T: Serialize>(value: &T, path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let json = match serde_json::to_string_pretty(value) {
        Ok(s) => s,
        Err(_) => return false,
    };
    std::fs::write(path, format!("{json}\n")).is_ok()
}

fn xdg_path(xdg_value: Option<&str>, home_value: Option<&str>, fallback_suffix: &str) -> PathBuf {
    if let Some(v) = xdg_value {
        if !v.is_empty() {
            return PathBuf::from(v).join(APP_NAME);
        }
    }
    if let Some(h) = home_value {
        if !h.is_empty() {
            return PathBuf::from(h).join(fallback_suffix).join(APP_NAME);
        }
    }
    PathBuf::from(fallback_suffix).join(APP_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let c = Config::default();
        assert_eq!(c.port, "");
        assert_eq!(c.brightness, 75);
        assert_eq!(c.keepalive_interval, 2);
        assert_eq!(c.fan_lcd_percent, 30);
    }

    #[test]
    fn display_state_default_values() {
        let s = DisplayState::default();
        assert!(s.media.is_empty());
        assert!(s.sysinfo_display.is_empty());
        assert_eq!(s.ratio, "2:1");
        assert_eq!(s.screen_mode, "Full Screen");
        assert_eq!(s.play_mode, "Single");
        assert_eq!(s.brightness, 75);
        assert_eq!(s.display_filter, "");
    }

    #[test]
    fn config_round_trips_through_json() {
        let cfg = Config {
            port: "/dev/ttyACM0".to_string(),
            brightness: 50,
            keepalive_interval: 5,
            fan_lcd_percent: 60,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn config_serializes_fan_lcd_percent_as_camel_case() {
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""fanLcdPercent""#));
        assert!(!json.contains(r#""fan_lcd_percent""#));
    }

    #[test]
    fn config_applies_defaults_for_missing_fields() {
        let json = r#"{"brightness":50}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.brightness, 50);
        assert_eq!(cfg.keepalive_interval, 2);
        assert_eq!(cfg.fan_lcd_percent, 30);
        assert_eq!(cfg.port, "");
    }

    #[test]
    fn config_applies_all_defaults_for_empty_object() {
        let cfg: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn display_state_round_trips_with_media_list() {
        let s = DisplayState {
            media: vec!["a.mp4".into(), "b.mp4".into()],
            media2: vec!["c.mp4".into()],
            sysinfo_display: vec!["CPU Temperature".into(), "GPU Temperature".into()],
            sysinfo_display2: vec!["GPU Usage".into()],
            badges: vec!["CPU Badge".into()],
            badges2: vec!["GPU Badge".into()],
            ratio: "1:1".into(),
            screen_mode: "Screen Splitting".into(),
            play_mode: "Loop".into(),
            brightness: 80,
            metrics_color: "#FF0000".into(),
            metrics_align: "Right".into(),
            metrics_position: "Bottom".into(),
            display_filter: "Smoke".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: DisplayState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, parsed);
    }

    #[test]
    fn display_state_applies_defaults_for_partial_input() {
        let json = r#"{"ratio":"4:3"}"#;
        let s: DisplayState = serde_json::from_str(json).unwrap();
        assert_eq!(s.ratio, "4:3");
        assert_eq!(s.screen_mode, "Full Screen");
        assert!(s.media.is_empty());
        assert!(s.media2.is_empty());
        assert!(s.sysinfo_display2.is_empty());
        assert_eq!(s.display_filter, "");
    }

    #[test]
    fn display_state_loads_old_files_without_newer_fields() {
        // Persisted state from before split-mode shipped — no media2 or
        // sysinfo_display2 keys. It also predates display_filter. Should
        // deserialize cleanly with empty defaults.
        let json = r#"{
            "media": ["clip.mp4"],
            "sysinfo_display": ["CPU Temperature"],
            "screen_mode": "Full Screen",
            "ratio": "2:1"
        }"#;
        let s: DisplayState = serde_json::from_str(json).unwrap();
        assert_eq!(s.media, vec!["clip.mp4".to_string()]);
        assert!(s.media2.is_empty());
        assert!(s.sysinfo_display2.is_empty());
        assert_eq!(s.display_filter, "");
    }

    #[test]
    fn xdg_prefers_xdg_value_when_set() {
        let p = xdg_path(Some("/x"), Some("/h"), ".config");
        assert_eq!(p, PathBuf::from("/x/tryx-panorama-mgr"));
    }

    #[test]
    fn xdg_falls_back_to_home_when_xdg_unset() {
        let p = xdg_path(None, Some("/h"), ".config");
        assert_eq!(p, PathBuf::from("/h/.config/tryx-panorama-mgr"));
    }

    #[test]
    fn xdg_falls_back_to_home_when_xdg_is_empty_string() {
        let p = xdg_path(Some(""), Some("/h"), ".config");
        assert_eq!(p, PathBuf::from("/h/.config/tryx-panorama-mgr"));
    }

    #[test]
    fn xdg_falls_back_to_bare_suffix_when_no_home() {
        let p = xdg_path(None, None, ".local/state");
        assert_eq!(p, PathBuf::from(".local/state/tryx-panorama-mgr"));
    }
}
