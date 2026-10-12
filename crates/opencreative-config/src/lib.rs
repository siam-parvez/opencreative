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
use opencreative_render::media::{Crop, Fit, Item, Source};
use opencreative_render::music::{self, BarStyle};
use opencreative_render::panels::{Layout, Skin};
pub use opencreative_sensors::Metric;
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
    #[error("a playlist item needs a `path` or an http(s) `url`, not both: {0}")]
    MediaSource(String),
    #[error("zoom must be between 1 and 8, not {0}")]
    Zoom(f32),
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
    Panels {
        #[serde(default)]
        skin: SkinConfig,
        primary: Metric,
        secondary: [Metric; 2],
        #[serde(default = "default_interval")]
        interval_secs: f64,
    },
    /// One image or GIF. Still read from older config files; the app now
    /// writes [`SceneConfig::Media`].
    Image {
        path: PathBuf,
        #[serde(default)]
        fit: FitConfig,
    },
    /// A playlist of images, GIFs and videos (files or web addresses).
    Media {
        items: Vec<MediaItem>,
        /// How long each item shows before the next, when there are several.
        #[serde(default = "default_item_secs")]
        item_secs: f64,
    },
    Solid {
        color: Rgb,
    },
    /// Bars that move with the music the computer is playing.
    Music {
        #[serde(default)]
        style: BarStyleConfig,
        #[serde(default = "default_music_color")]
        color: Rgb,
        /// Color each bar by its pitch instead of `color`.
        #[serde(default)]
        rainbow: bool,
        #[serde(default = "default_bars")]
        bars: u8,
    },
}

fn default_music_color() -> Rgb {
    Rgb([0x22, 0xd3, 0xee])
}

fn default_bars() -> u8 {
    24
}

/// How music bars are laid out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarStyleConfig {
    /// Rising from the bottom.
    #[default]
    Bars,
    /// Growing up and down from the middle.
    Mirror,
}

impl From<BarStyleConfig> for BarStyle {
    fn from(s: BarStyleConfig) -> Self {
        match s {
            BarStyleConfig::Bars => BarStyle::Bars,
            BarStyleConfig::Mirror => BarStyle::Mirror,
        }
    }
}

impl SceneConfig {
    /// The default Music scene.
    pub fn music() -> Self {
        Self::Music {
            style: BarStyleConfig::default(),
            color: default_music_color(),
            rainbow: false,
            bars: default_bars(),
        }
    }
}

fn default_interval() -> f64 {
    1.0
}

fn default_item_secs() -> f64 {
    10.0
}

/// A playlist entry: a file or a web address, with how to fit and crop it.
///
/// ```toml
/// [[scene.items]]
/// path = "/home/me/cat.gif"
/// fit = "cover"
/// zoom = 1.5
/// focus = [0.5, 0.3]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub fit: FitConfig,
    /// 1 shows the whole fitted picture; 2 shows half of it, and so on.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub zoom: f32,
    /// The point to zoom in on, as fractions of the width and height.
    #[serde(default = "middle", skip_serializing_if = "is_middle")]
    pub focus: [f32; 2],
}

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    *v == 1.0
}

fn middle() -> [f32; 2] {
    [0.5, 0.5]
}

fn is_middle(v: &[f32; 2]) -> bool {
    *v == [0.5, 0.5]
}

impl MediaItem {
    pub fn file(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            url: None,
            fit: FitConfig::default(),
            zoom: 1.0,
            focus: middle(),
        }
    }

    pub fn web(url: String) -> Self {
        Self {
            path: None,
            url: Some(url),
            ..Self::file(PathBuf::new())
        }
    }

    /// A short name for lists: the file name, or the address.
    pub fn label(&self) -> String {
        match (&self.path, &self.url) {
            (Some(path), _) => path.file_name().map_or_else(
                || path.display().to_string(),
                |f| f.to_string_lossy().into_owned(),
            ),
            (None, Some(url)) => url.clone(),
            (None, None) => "(nothing)".to_owned(),
        }
    }

    pub fn to_item(&self) -> std::result::Result<Item, InvalidValue> {
        let source = match (&self.path, &self.url) {
            (Some(path), None) => Source::Path(path.clone()),
            (None, Some(url)) if url.starts_with("http://") || url.starts_with("https://") => {
                Source::Url(url.clone())
            }
            _ => return Err(InvalidValue::MediaSource(self.label())),
        };
        if !(1.0..=Crop::MAX_ZOOM).contains(&self.zoom) {
            return Err(InvalidValue::Zoom(self.zoom));
        }
        Ok(Item {
            source,
            fit: self.fit.into(),
            crop: Crop {
                zoom: self.zoom,
                x: self.focus[0].clamp(0.0, 1.0),
                y: self.focus[1].clamp(0.0, 1.0),
            },
        })
    }
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self::Panels {
            skin: SkinConfig::Lines,
            primary: Metric::CpuTemp,
            secondary: [Metric::CpuFreq, Metric::CpuLoad],
            interval_secs: default_interval(),
        }
    }
}

impl SceneConfig {
    pub fn to_scene(&self) -> std::result::Result<Scene, InvalidValue> {
        let interval = |secs: f64| {
            if secs.is_finite() && secs > 0.0 {
                Ok(Duration::from_secs_f64(secs))
            } else {
                Err(InvalidValue::Interval(secs))
            }
        };
        Ok(match self {
            Self::Monitor { interval_secs } => Scene::Monitor {
                interval: interval(*interval_secs)?,
            },
            Self::Panels {
                skin,
                primary,
                secondary,
                interval_secs,
            } => Scene::Panels {
                skin: (*skin).into(),
                layout: Layout {
                    primary: *primary,
                    secondary: *secondary,
                },
                interval: interval(*interval_secs)?,
            },
            Self::Image { path, fit } => Scene::Media {
                items: vec![Item {
                    source: Source::Path(path.clone()),
                    fit: (*fit).into(),
                    crop: Crop::default(),
                }],
                item_time: Duration::from_secs(10),
            },
            Self::Media { items, item_secs } => Scene::Media {
                items: items
                    .iter()
                    .map(MediaItem::to_item)
                    .collect::<std::result::Result<_, _>>()?,
                item_time: interval(*item_secs)?,
            },
            Self::Solid { color } => Scene::Solid(color.0),
            Self::Music {
                style,
                color,
                rainbow,
                bars,
            } => Scene::Music {
                look: music::Look {
                    style: (*style).into(),
                    color: color.0,
                    rainbow: *rainbow,
                },
                bars: (*bars).clamp(4, 64) as usize,
            },
        })
    }
}

/// Look of the Panels screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinConfig {
    /// Thin interlocking lines over a tile grid.
    #[default]
    Lines,
    /// Neon frames with live charts.
    Hud,
}

impl SkinConfig {
    pub const ALL: [SkinConfig; 2] = [SkinConfig::Lines, SkinConfig::Hud];

    pub fn name(self) -> &'static str {
        match self {
            SkinConfig::Lines => "Lines",
            SkinConfig::Hud => "HUD",
        }
    }
}

impl From<SkinConfig> for Skin {
    fn from(s: SkinConfig) -> Self {
        match s {
            SkinConfig::Lines => Skin::Lines,
            SkinConfig::Hud => Skin::Hud,
        }
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
                items: vec![Item {
                    source: Source::Path("/tmp/a.gif".into()),
                    fit: Fit::Contain,
                    crop: Crop::default(),
                }],
                item_time: Duration::from_secs(10),
            }
        );
    }

    #[test]
    fn playlists_parse_and_old_image_scenes_still_load() {
        let text = r#"
            [scene]
            type = "media"
            item_secs = 5.0

            [[scene.items]]
            path = "/tmp/cat.gif"
            zoom = 2.0
            focus = [0.25, 0.5]

            [[scene.items]]
            url = "https://www.youtube.com/watch?v=x"
            fit = "contain"
        "#;
        let config: Config = toml::from_str(text).unwrap();
        let Scene::Media { items, item_time } = config.scene.to_scene().unwrap() else {
            panic!("expected media");
        };
        assert_eq!(item_time, Duration::from_secs(5));
        assert_eq!(items[0].crop.zoom, 2.0);
        assert_eq!(items[0].crop.x, 0.25);
        assert!(matches!(items[1].source, Source::Url(_)));
        assert_eq!(items[1].fit, Fit::Contain);

        // Defaults stay out of the file.
        let saved = toml::to_string(&SceneConfig::Media {
            items: vec![MediaItem::file("/tmp/a.png".into())],
            item_secs: 10.0,
        })
        .unwrap();
        assert!(
            !saved.contains("zoom") && !saved.contains("focus"),
            "{saved}"
        );

        let old: Config =
            toml::from_str("[scene]\ntype = \"image\"\npath = \"/tmp/a.png\"").unwrap();
        assert!(matches!(old.scene.to_scene().unwrap(), Scene::Media { .. }));

        let bad = |item: MediaItem| {
            SceneConfig::Media {
                items: vec![item],
                item_secs: 10.0,
            }
            .to_scene()
            .is_err()
        };
        assert!(bad(MediaItem::web("file:///etc/passwd".into())));
        assert!(bad(MediaItem {
            zoom: 9.0,
            ..MediaItem::file("/tmp/a.png".into())
        }));
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
