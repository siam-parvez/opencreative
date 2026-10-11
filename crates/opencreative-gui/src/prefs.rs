//! App-only preferences (not used by the daemon), stored next to the
//! daemon's config as `gui.toml`.

use std::fs;
use std::path::PathBuf;

use eframe::egui::ThemePreference;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    /// Follow the desktop's light/dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn preference(self) -> ThemePreference {
        match self {
            Appearance::System => ThemePreference::System,
            Appearance::Light => ThemePreference::Light,
            Appearance::Dark => ThemePreference::Dark,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub appearance: Appearance,
    /// Show Claude usage from OpenCUT (off unless the user turns it on).
    pub claude_usage: bool,
    /// Whether first-launch setup (starting with the computer) has run.
    pub first_run_done: bool,
}

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("opencreative").join("gui.toml"))
}

/// Missing or unreadable preferences fall back to defaults.
pub fn load() -> Prefs {
    path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(prefs: &Prefs) -> Result<(), String> {
    let path = path().ok_or("no config directory")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let prefs = Prefs {
            appearance: Appearance::Dark,
            claude_usage: true,
            first_run_done: true,
        };
        let text = toml::to_string_pretty(&prefs).unwrap();
        assert!(text.contains("appearance = \"dark\""));
        assert_eq!(toml::from_str::<Prefs>(&text).unwrap(), prefs);
        assert_eq!(toml::from_str::<Prefs>("").unwrap(), Prefs::default());
    }
}
