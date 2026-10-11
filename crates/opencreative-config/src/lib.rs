//! Persistent settings, stored as TOML in the user's config directory
//! (`~/.config/opencreative/config.toml` on Linux).
//!
//! ```toml
//! rotation = 0
//! brightness = 60
//!
//! [scene]
//! type = "monitor"
//! interval_secs = 1.0
//! ```

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use opencreative_core::Scene;
use opencreative_render::Rotation;
use opencreative_render::media::Fit;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no config directory on this system")]
    NoConfigDir,
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error(transparent)]
    Invalid(#[from] InvalidValue),
}

#[derive(Debug, Error, PartialEq)]
pub enum InvalidValue {
    #[error("rotation must be 0, 90, 180 or 270, not {0}")]
    Rotation(u16),
    #[error("brightness must be 0-100, not {0}")]
    Brightness(u8),
    #[error("interval must be a positive number of seconds, not {0}")]
    Interval(f64),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Device to drive; the first one found when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Clockwise degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Backlight percentage applied on connect; left alone when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    pub scene: SceneConfig,
}

impl Config {
    pub fn rotation(&self) -> std::result::Result<Rotation, InvalidValue> {
        Rotation::from_degrees(self.rotation).ok_or(InvalidValue::Rotation(self.rotation))
    }

    /// Checks values serde can't: ranges and enumerations.
    pub fn validate(&self) -> std::result::Result<(), InvalidValue> {
        self.rotation()?;
        if let Some(b) = self.brightness.filter(|b| *b > 100) {
            return Err(InvalidValue::Brightness(b));
        }
        self.scene.to_scene()?;
        Ok(())
    }
}

/// What the screen shows, in a serializable form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneConfig {
    Monitor {
        interval_secs: f64,
    },
    Image {
        path: PathBuf,
        #[serde(default)]
        fit: FitConfig,
    },
    Solid {
        color: Rgb,
    },
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self::Monitor { interval_secs: 1.0 }
    }
}

impl SceneConfig {
    pub fn to_scene(&self) -> std::result::Result<Scene, InvalidValue> {
        Ok(match self {
            Self::Monitor { interval_secs } => {
                if !(interval_secs.is_finite() && *interval_secs > 0.0) {
                    return Err(InvalidValue::Interval(*interval_secs));
                }
                Scene::Monitor {
                    interval: Duration::from_secs_f64(*interval_secs),
                }
            }
            Self::Image { path, fit } => Scene::Media {
                path: path.clone(),
                fit: (*fit).into(),
            },
            Self::Solid { color } => Scene::Solid(color.0),
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitConfig {
    #[default]
    Cover,
    Contain,
    Stretch,
}

impl From<FitConfig> for Fit {
    fn from(f: FitConfig) -> Self {
        match f {
            FitConfig::Cover => Fit::Cover,
            FitConfig::Contain => Fit::Contain,
            FitConfig::Stretch => Fit::Stretch,
        }
    }
}

/// An RGB color written as `"#rrggbb"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb(pub [u8; 3]);

impl std::str::FromStr for Rgb {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let hex = s.trim_start_matches('#');
        if hex.len() != 6 {
            return Err("expected a color like #ff8800".to_owned());
        }
        let v = u32::from_str_radix(hex, 16).map_err(|_| format!("invalid hex color: {s}"))?;
        Ok(Self([(v >> 16) as u8, (v >> 8) as u8, v as u8]))
    }
}

impl TryFrom<String> for Rgb {
    type Error = String;

    fn try_from(s: String) -> std::result::Result<Self, String> {
        s.parse()
    }
}

impl From<Rgb> for String {
    fn from(c: Rgb) -> Self {
        c.to_string()
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")
    }
}

/// Default config file location.
pub fn default_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or(Error::NoConfigDir)?
        .join("opencreative")
        .join("config.toml"))
}

/// Loads and validates a config file. A missing file yields the defaults.
pub fn load(path: &Path) -> Result<Config> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(source) => {
            return Err(Error::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    let config: Config = toml::from_str(&text).map_err(|source| Error::Parse {
        path: path.to_owned(),
        source,
    })?;
    config.validate()?;
    Ok(config)
}

/// Writes a config file atomically (write to a temporary file, then rename),
/// creating the directory if needed.
pub fn save(path: &Path, config: &Config) -> Result<()> {
    let io_err = |source| Error::Io {
        path: path.to_owned(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err)?;
    }
    let text = toml::to_string_pretty(config).expect("config always serializes");
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("opencreative-test-{}-{name}", std::process::id()))
            .join("config.toml")
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(load(&temp_path("missing")).unwrap(), Config::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let path = temp_path("roundtrip");
        let config = Config {
            serial: Some("ABC".into()),
            rotation: 180,
            brightness: Some(60),
            scene: SceneConfig::Solid {
                color: Rgb([0xff, 0x88, 0x00]),
            },
        };
        save(&path, &config).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("\"#ff8800\""));
        assert_eq!(load(&path).unwrap(), config);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn parses_documented_example() {
        let config: Config = toml::from_str(
            "rotation = 90\n[scene]\ntype = \"image\"\npath = \"/tmp/a.gif\"\nfit = \"contain\"\n",
        )
        .unwrap();
        assert_eq!(config.rotation().unwrap(), Rotation::R90);
        assert_eq!(
            config.scene.to_scene().unwrap(),
            Scene::Media {
                path: "/tmp/a.gif".into(),
                fit: Fit::Contain
            }
        );
    }

    #[test]
    fn rejects_bad_values() {
        let bad = |config: Config| config.validate().unwrap_err();
        let base = Config::default();
        assert_eq!(
            bad(Config {
                rotation: 45,
                ..base.clone()
            }),
            InvalidValue::Rotation(45)
        );
        assert_eq!(
            bad(Config {
                brightness: Some(101),
                ..base.clone()
            }),
            InvalidValue::Brightness(101)
        );
        assert_eq!(
            bad(Config {
                scene: SceneConfig::Monitor { interval_secs: 0.0 },
                ..base
            }),
            InvalidValue::Interval(0.0)
        );
        assert!(toml::from_str::<Config>("bogus = 1").is_err());
    }

    #[test]
    fn colors_parse_and_print() {
        assert_eq!("#ff8800".parse::<Rgb>(), Ok(Rgb([0xff, 0x88, 0])));
        assert_eq!("00ff00".parse::<Rgb>(), Ok(Rgb([0, 0xff, 0])));
        assert!("#fff".parse::<Rgb>().is_err());
        assert_eq!(Rgb([1, 2, 3]).to_string(), "#010203");
    }
}
