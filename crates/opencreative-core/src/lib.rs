//! Scene playback shared by every frontend (CLI today; daemon and GUI next).
//!
//! A [`Player`] decides which frame to show next for a [`Scene`]; [`run`]
//! keeps those frames on the device, reconnecting when it disappears, and
//! [`run_daemon`] does the same while taking [`Command`]s.

mod daemon;

pub use daemon::{Command, DaemonState, run_daemon};

use std::collections::VecDeque;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use opencreative_device::{self as device, Display};
use opencreative_render::media::{self, Fit};
use opencreative_render::{Pixmap, Rotation, monitor, panels, rotate, solid, zen};
use opencreative_sensors::Sensors;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Device(#[from] device::Error),
    #[error("loading {path}: {source}")]
    Media {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("{0} has no frames")]
    EmptyMedia(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;

/// What the screen shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Scene {
    /// Live system monitor (gauges), refreshed every `interval`.
    Monitor {
        interval: Duration,
    },
    /// Chosen readings in outlined panels, refreshed every `interval`.
    Panels {
        layout: panels::Layout,
        skin: panels::Skin,
        interval: Duration,
    },
    /// A still image or animated GIF.
    Media {
        path: PathBuf,
        fit: Fit,
    },
    Solid([u8; 3]),
    /// A calm screen with a dim clock (Zen mode).
    Zen,
}

impl Scene {
    /// Whether playback should wait for a device that isn't plugged in yet
    /// (a background monitor) rather than fail right away (a one-off command).
    fn waits_for_device(&self) -> bool {
        self.is_live()
    }

    /// Whether the scene shows sensor readings.
    fn is_live(&self) -> bool {
        matches!(self, Self::Monitor { .. } | Self::Panels { .. } | Self::Zen)
    }
}

/// A frame in panel orientation and how long to show it. `None` means it
/// never needs refreshing.
pub struct Output {
    pub frame: Pixmap,
    pub hold: Option<Duration>,
}

/// Decoded media frames, rotated, and the panel size they were loaded for.
struct LoadedMedia {
    size: (u32, u32),
    frames: Vec<(Pixmap, Duration)>,
}

/// Produces frames for a scene.
pub struct Player {
    scene: Scene,
    rotation: Rotation,
    sensors: Option<Sensors>,
    media: Option<LoadedMedia>,
    cursor: usize,
    /// Recent readings, oldest first, for skins that chart them.
    history: VecDeque<opencreative_sensors::Snapshot>,
    /// Whether this player turned Claude usage polling on.
    claude_on: bool,
}

/// How many readings a chart covers (a minute at the default rate).
const HISTORY: usize = 60;

impl Player {
    pub fn new(scene: Scene, rotation: Rotation) -> Self {
        let sensors = scene.is_live().then(Sensors::new);
        Self {
            scene,
            rotation,
            sensors,
            media: None,
            cursor: 0,
            history: VecDeque::with_capacity(HISTORY),
            claude_on: false,
        }
    }

    /// Polls Claude usage only while a shown reading needs it.
    fn want_claude(&mut self, on: bool) {
        if on != self.claude_on {
            opencreative_sensors::set_claude_enabled(on);
            self.claude_on = on;
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// Renders the next frame for a `panel_w` x `panel_h` panel.
    pub fn next(&mut self, panel_w: u32, panel_h: u32) -> Result<Output> {
        let (w, h) = self.rotation.logical_size(panel_w, panel_h);
        let claude = match &self.scene {
            Scene::Panels { layout, .. } => {
                layout.primary.is_claude() || layout.secondary.iter().any(|m| m.is_claude())
            }
            _ => false,
        };
        self.want_claude(claude);
        match &self.scene {
            Scene::Monitor { interval } => {
                let snap = self.sensors.get_or_insert_with(Sensors::new).read();
                let clock = chrono::Local::now().format("%H:%M").to_string();
                Ok(Output {
                    frame: rotate(&monitor::render(&snap, &clock, w, h), self.rotation),
                    hold: Some(*interval),
                })
            }
            Scene::Panels {
                layout,
                skin,
                interval,
            } => {
                let snap = self.sensors.get_or_insert_with(Sensors::new).read();
                if self.history.len() == HISTORY {
                    self.history.pop_front();
                }
                self.history.push_back(snap.clone());
                let history = self.history.make_contiguous();
                let frame = panels::render(&snap, history, layout, *skin, w, h);
                Ok(Output {
                    frame: rotate(&frame, self.rotation),
                    hold: Some(*interval),
                })
            }
            Scene::Zen => {
                let snap = self.sensors.get_or_insert_with(Sensors::new).read();
                let clock = chrono::Local::now().format("%H:%M").to_string();
                Ok(Output {
                    frame: rotate(&zen::render(&snap, &clock, w, h), self.rotation),
                    hold: Some(Duration::from_secs(1)),
                })
            }
            Scene::Solid(rgb) => Ok(Output {
                frame: rotate(&solid(w, h, *rgb), self.rotation),
                hold: None,
            }),
            Scene::Media { path, fit } => {
                if self.media.as_ref().is_none_or(|m| m.size != (w, h)) {
                    let frames = media::load(path, w, h, *fit).map_err(|source| Error::Media {
                        path: path.clone(),
                        source,
                    })?;
                    if frames.is_empty() {
                        return Err(Error::EmptyMedia(path.clone()));
                    }
                    // Rotate once up front so playback is just USB writes.
                    let frames = frames
                        .into_iter()
                        .map(|f| (rotate(&f.pixmap, self.rotation), f.delay))
                        .collect();
                    self.media = Some(LoadedMedia {
                        size: (w, h),
                        frames,
                    });
                    self.cursor = 0;
                }
                let frames = &self.media.as_ref().expect("loaded above").frames;
                let (frame, delay) = &frames[self.cursor % frames.len()];
                self.cursor = (self.cursor + 1) % frames.len();
                Ok(Output {
                    frame: frame.clone(),
                    hold: (frames.len() > 1).then_some(*delay),
                })
            }
        }
    }
}

/// How long to wait before retrying after the device disappears (e.g. a
/// replug or suspend/resume).
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Shows the player's scene on the device with the given serial (or the first
/// one found). Returns after a still frame is shown; animated scenes run until
/// an error that retrying can't fix.
pub fn run(player: &mut Player, serial: Option<&str>) -> Result<()> {
    let mut dev: Option<Box<dyn Display>> = None;
    let mut connected_once = false;

    loop {
        if dev.is_none() {
            match device::open(serial) {
                Ok(d) => {
                    log::info!("connected to {}", d.spec().name);
                    dev = Some(d);
                    connected_once = true;
                }
                Err(e @ device::Error::PermissionDenied) => return Err(e.into()),
                Err(e) if connected_once || player.scene().waits_for_device() => {
                    log::warn!("{e}; retrying in {RECONNECT_DELAY:?}");
                    thread::sleep(RECONNECT_DELAY);
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        let d = dev.as_mut().expect("connected above");

        let started = Instant::now();
        let spec = d.spec();
        let out = player.next(spec.width, spec.height)?;
        if let Err(e) = d.send_frame(out.frame.data()) {
            log::warn!("lost device: {e}");
            dev = None;
            continue;
        }
        match out.hold {
            None => return Ok(()),
            Some(hold) => thread::sleep(hold.saturating_sub(started.elapsed())),
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.want_claude(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_is_a_single_still_frame_in_panel_orientation() {
        let mut p = Player::new(Scene::Solid([255, 0, 0]), Rotation::R90);
        let out = p.next(320, 240).unwrap();
        assert_eq!((out.frame.width(), out.frame.height()), (320, 240));
        assert!(out.hold.is_none());
        assert_eq!(&out.frame.data()[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn monitor_refreshes_on_its_interval() {
        let interval = Duration::from_millis(250);
        let mut p = Player::new(Scene::Monitor { interval }, Rotation::R0);
        let out = p.next(320, 240).unwrap();
        assert_eq!(out.hold, Some(interval));
        assert_eq!((out.frame.width(), out.frame.height()), (320, 240));
    }

    #[test]
    fn missing_media_reports_the_path() {
        let path = PathBuf::from("/nonexistent/opencreative.png");
        let mut p = Player::new(
            Scene::Media {
                path: path.clone(),
                fit: Fit::Cover,
            },
            Rotation::R0,
        );
        let Err(Error::Media { path: got, .. }) = p.next(320, 240) else {
            panic!("expected a media error");
        };
        assert_eq!(got, path);
    }

    #[test]
    fn only_the_monitor_waits_for_a_device() {
        let monitor = Scene::Monitor {
            interval: Duration::from_secs(1),
        };
        assert!(monitor.waits_for_device());
        assert!(!Scene::Solid([0, 0, 0]).waits_for_device());
    }
}
