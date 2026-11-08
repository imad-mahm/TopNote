use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const DEFAULT_SECRETS_URL: &str =
    "https://raw.githubusercontent.com/Thereallo1026/spotify-secrets/refs/heads/main/secrets/secretDict.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shortcuts {
    pub toggle_window: String,
    pub click_through: String,
    pub toggle_view: String,
    pub offset_up: String,
    pub offset_down: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            toggle_window: "ctrl+alt+m".into(),
            click_through: "ctrl+alt+l".into(),
            toggle_view: "ctrl+alt+v".into(),
            offset_up: "ctrl+alt+up".into(),
            offset_down: "ctrl+alt+down".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub client_id: String,
    pub onboarded: bool,
    pub provider_order: Vec<String>,
    pub offset_ms: i64,
    pub font_scale: f64,
    pub word_sync: bool,
    pub opacity: f64,
    pub blur_background: bool,
    /// "artwork" | "spotify" | "fixed"
    pub accent_mode: String,
    pub fixed_accent: String,
    pub skip_taskbar: bool,
    pub autostart: bool,
    pub snap_edges: bool,
    /// "artwork" | "lyrics"
    pub view: String,
    pub spotify_secrets_url: String,
    pub shortcuts: Shortcuts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            onboarded: false,
            provider_order: vec!["spotify".into(), "musixmatch".into(), "lrclib".into()],
            offset_ms: 0,
            font_scale: 1.0,
            word_sync: true,
            opacity: 1.0,
            blur_background: true,
            accent_mode: "artwork".into(),
            fixed_accent: "#8ee6a8".into(),
            skip_taskbar: false,
            autostart: false,
            snap_edges: true,
            view: "artwork".into(),
            spotify_secrets_url: DEFAULT_SECRETS_URL.into(),
            shortcuts: Shortcuts::default(),
        }
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read_to_string(path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, s: &Settings) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(path(dir), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// Shallow-merge a JSON patch into settings (nested objects merged one level deep).
pub fn merge(current: &Settings, patch: &Value) -> Result<Settings> {
    let mut v = serde_json::to_value(current)?;
    if let (Some(dst), Some(src)) = (v.as_object_mut(), patch.as_object()) {
        for (k, val) in src {
            match (dst.get_mut(k), val) {
                (Some(Value::Object(d)), Value::Object(s)) => {
                    for (k2, v2) in s {
                        d.insert(k2.clone(), v2.clone());
                    }
                }
                _ => {
                    dst.insert(k.clone(), val.clone());
                }
            }
        }
    }
    Ok(serde_json::from_value(v)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merge_patch() {
        let s = Settings::default();
        let m = merge(&s, &serde_json::json!({"offsetMs": 150, "shortcuts": {"toggleView": "ctrl+shift+v"}})).unwrap();
        assert_eq!(m.offset_ms, 150);
        assert_eq!(m.shortcuts.toggle_view, "ctrl+shift+v");
        assert_eq!(m.shortcuts.toggle_window, "ctrl+alt+m");
    }
}
