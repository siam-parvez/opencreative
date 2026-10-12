//! Scene playback shared by every frontend (CLI today; daemon and GUI next).
//!
//! A [`Player`] decides which frame to show next for a [`Scene`]; [`run`]
//! keeps those frames on the device, reconnecting when it disappears, and
//! [`run_daemon`] does the same while taking [`Command`]s.

mod daemon;

pub use daemon::{Command, DaemonState, SentFrame, run_daemon};

use std::collections::VecDeque;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use opencreative_device::{self as device, Display};
use opencreative_render::media::{self, Item, Source};
use opencreative_render::video::{self, Stream};
use opencreative_render::{
    Pixmap, Rotation, Shade, monitor, music, panels, rotate, shade, solid, zen,
};
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
    #[error(transparent)]
    Video(#[from] video::VideoError),
    #[error("the playlist is empty")]
    EmptyPlaylist,
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
    /// Images, GIFs and videos, each shown for `item_time` in turn (a single
    /// item stays up for good).
    Media {
        items: Vec<Item>,
        item_time: Duration,
    },
    Solid([u8; 3]),
    /// Bars that move with what the computer is playing.
    Music {
        look: music::Look,
        bars: usize,
    },
    /// A calm screen with a dim clock (Zen mode).
    Zen,
}

impl Scene {
    /// Whether playback should wait for a device that isn't plugged in yet
    /// (a background monitor) rather than fail right away (a one-off command).
    fn waits_for_device(&self) -> bool {
        self.is_live() || matches!(self, Self::Music { .. })
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

/// The playlist item on screen, ready to play at the panel size it was
/// loaded for.
struct LoadedMedia {
    index: usize,
    size: (u32, u32),
    started: Instant,
    content: Content,
}

enum Content {
    /// Decoded image or GIF frames, rotated, with their delays.
    Frames {
        frames: Vec<(Pixmap, Duration)>,
        cursor: usize,
    },
    /// A video being decoded as it plays.
    Video(Stream),
}

/// Produces frames for a scene.
pub struct Player {
    scene: Scene,
    rotation: Rotation,
    /// Dark or light, for readings, music bars and Zen (not media).
    shade: Shade,
    sensors: Option<Sensors>,
    media: Option<LoadedMedia>,
    /// Recent readings, oldest first, for skins that chart them.
    history: VecDeque<opencreative_sensors::Snapshot>,
    /// Whether this player turned Claude usage polling on.
    claude_on: bool,
    /// Recording what's playing, for the Music scene.
    audio: Option<Audio>,
}

/// The Music scene's recorder, or why there isn't one (retried now and
/// then, e.g. until the sound server is up after login).
enum Audio {
    Recording(opencreative_audio::Spectrum),
    Failed { why: String, at: Instant },
}

/// The Music scene's frame rate.
const MUSIC_FRAME: Duration = Duration::from_millis(33);
const AUDIO_RETRY: Duration = Duration::from_secs(5);

/// How many readings a chart covers (a minute at the default rate).
const HISTORY: usize = 60;

impl Player {
    pub fn new(scene: Scene, rotation: Rotation) -> Self {
        let sensors = scene.is_live().then(Sensors::new);
        Self {
            scene,
            rotation,
            shade: Shade::Dark,
            sensors,
            media: None,
            history: VecDeque::with_capacity(HISTORY),
            claude_on: false,
            audio: None,
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

    /// This player with screens drawn in `shade`.
    pub fn with_shade(mut self, shade: Shade) -> Self {
        self.shade = shade;
        self
    }

    pub fn shade(&self) -> Shade {
        self.shade
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
                let mut frame = monitor::render(&snap, &clock, w, h);
                shade::apply(&mut frame, self.shade);
                Ok(Output {
                    frame: rotate(&frame, self.rotation),
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
                let mut frame = panels::render(&snap, history, layout, *skin, w, h);
                shade::apply(&mut frame, self.shade);
                Ok(Output {
                    frame: rotate(&frame, self.rotation),
                    hold: Some(*interval),
                })
            }
            Scene::Zen => {
                let snap = self.sensors.get_or_insert_with(Sensors::new).read();
                let clock = chrono::Local::now().format("%H:%M").to_string();
                let mut frame = zen::render(&snap, &clock, w, h);
                shade::apply(&mut frame, self.shade);
                Ok(Output {
                    frame: rotate(&frame, self.rotation),
                    hold: Some(Duration::from_secs(1)),
                })
            }
            Scene::Solid(rgb) => Ok(Output {
                frame: rotate(&solid(w, h, *rgb), self.rotation),
                hold: None,
            }),
            Scene::Media { items, item_time } => {
                let (items, item_time) = (items.clone(), *item_time);
                self.next_media(&items, item_time, (w, h))
            }
            Scene::Music { look, bars } => {
                let (look, bars) = (*look, *bars);
                let (bands, message) = self.listen(bars);
                let mut frame =
                    music::render(&bands.levels, &bands.peaks, &look, message.as_deref(), w, h);
                shade::apply(&mut frame, self.shade);
                Ok(Output {
                    frame: rotate(&frame, self.rotation),
                    hold: Some(MUSIC_FRAME),
                })
            }
        }
    }

    /// The current spectrum, starting the recorder if needed; when it can't
    /// record, flat bars and a short note why.
    fn listen(&mut self, bars: usize) -> (opencreative_audio::Bands, Option<String>) {
        let flat = || opencreative_audio::Bands {
            levels: vec![0.0; bars],
            peaks: vec![0.0; bars],
        };
        let retry = match &self.audio {
            None => true,
            Some(Audio::Failed { at, .. }) => at.elapsed() >= AUDIO_RETRY,
            Some(Audio::Recording(_)) => false,
        };
        if retry {
            self.audio = Some(match opencreative_audio::Spectrum::start() {
                Ok(spectrum) => Audio::Recording(spectrum),
                Err(e) => {
                    log::warn!("music bars: {e}");
                    Audio::Failed {
                        why: e.to_string(),
                        at: Instant::now(),
                    }
                }
            });
        }
        match &mut self.audio {
            Some(Audio::Recording(spectrum)) => match spectrum.bands(bars) {
                Ok(bands) => (bands, None),
                Err(e) => {
                    log::warn!("music bars: {e}");
                    self.audio = Some(Audio::Failed {
                        why: e.to_string(),
                        at: Instant::now(),
                    });
                    (flat(), Some("NO AUDIO".to_owned()))
                }
            },
            Some(Audio::Failed { why, .. }) => {
                log::debug!("music bars: {why}");
                (flat(), Some("NO AUDIO".to_owned()))
            }
            None => (flat(), None),
        }
    }

    /// The next playlist frame. Moves to the next item when this one's time
    /// is up; an item that fails to load is skipped unless it's the only one.
    fn next_media(
        &mut self,
        items: &[Item],
        item_time: Duration,
        size: (u32, u32),
    ) -> Result<Output> {
        if items.is_empty() {
            return Err(Error::EmptyPlaylist);
        }
        let mut index = match &self.media {
            Some(m) if m.size == size => {
                let due = items.len() > 1 && m.started.elapsed() >= item_time;
                if due {
                    (m.index + 1) % items.len()
                } else {
                    m.index % items.len()
                }
            }
            _ => 0,
        };
        let reload = self
            .media
            .as_ref()
            .is_none_or(|m| m.size != size || m.index != index);
        if reload {
            self.media = None;
            let mut last_error = None;
            for _ in 0..items.len() {
                match self.load_item(&items[index], size) {
                    Ok(content) => {
                        self.media = Some(LoadedMedia {
                            index,
                            size,
                            started: Instant::now(),
                            content,
                        });
                        break;
                    }
                    Err(e) => {
                        log::warn!("skipping playlist item: {e}");
                        last_error = Some(e);
                        index = (index + 1) % items.len();
                    }
                }
            }
            if self.media.is_none() {
                return Err(last_error.unwrap_or(Error::EmptyPlaylist));
            }
        }
        let rotation = self.rotation;
        let media = self.media.as_mut().expect("loaded above");
        // Time left before the next item, if there is one.
        let left = (items.len() > 1).then(|| item_time.saturating_sub(media.started.elapsed()));
        match &mut media.content {
            Content::Frames { frames, cursor } => {
                let (frame, delay) = &frames[*cursor % frames.len()];
                *cursor = (*cursor + 1) % frames.len();
                let hold = match (frames.len() > 1, left) {
                    (true, Some(left)) => Some((*delay).min(left)),
                    (true, None) => Some(*delay),
                    (false, left) => left,
                };
                Ok(Output {
                    frame: frame.clone(),
                    hold: hold.map(|h| h.max(Duration::from_millis(10))),
                })
            }
            Content::Video(stream) => {
                // At the end, loop; a web video that drops out (sites
                // refuse requests now and then) gets a few more tries.
                let mut frame = stream.next_frame();
                for _ in 0..3 {
                    if frame.is_some() {
                        break;
                    }
                    *stream = Stream::open(&items[media.index], size.0, size.1)?;
                    frame = stream.next_frame();
                }
                let frame = frame.ok_or_else(|| Error::Video(stream.failure()))?;
                Ok(Output {
                    frame: rotate(&frame, rotation),
                    hold: Some(Stream::frame_time()),
                })
            }
        }
    }

    fn load_item(&self, item: &Item, (w, h): (u32, u32)) -> Result<Content> {
        if item.is_video() {
            return Ok(Content::Video(Stream::open(item, w, h)?));
        }
        let Source::Path(path) = &item.source else {
            unreachable!("web addresses are videos");
        };
        let frames =
            media::load(path, w, h, item.fit, item.crop).map_err(|source| Error::Media {
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
        Ok(Content::Frames { frames, cursor: 0 })
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
                items: vec![Item {
                    source: Source::Path(path.clone()),
                    fit: media::Fit::Cover,
                    crop: media::Crop::default(),
                }],
                item_time: Duration::from_secs(10),
            },
            Rotation::R0,
        );
        let Err(Error::Media { path: got, .. }) = p.next(320, 240) else {
            panic!("expected a media error");
        };
        assert_eq!(got, path);
    }

    /// Needs ffmpeg on PATH; skipped (passes) without it.
    #[test]
    fn plays_a_video_and_moves_through_the_playlist() {
        let Some(ffmpeg) = video::tool("ffmpeg") else {
            eprintln!("ffmpeg not found; skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("oc-video-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.mp4");
        let made = std::process::Command::new(ffmpeg)
            .args(["-loglevel", "error", "-y", "-f", "lavfi"])
            .args(["-i", "color=c=red:s=640x360:d=0.5:r=25"])
            .args(["-pix_fmt", "yuv420p"])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(made.success());
        let item = |source| Item {
            source,
            fit: media::Fit::Cover,
            crop: media::Crop::default(),
        };
        let mut p = Player::new(
            Scene::Media {
                items: vec![
                    item(Source::Path(clip.clone())),
                    item(Source::Path("/nonexistent.png".into())),
                ],
                item_time: Duration::from_millis(300),
            },
            Rotation::R0,
        );
        // Ten frames is longer than the clip: it loops.
        for _ in 0..12 {
            let out = p.next(320, 240).unwrap();
            assert_eq!((out.frame.width(), out.frame.height()), (320, 240));
            assert_eq!(out.hold, Some(Stream::frame_time()));
            let px = &out.frame.data()[(120 * 320 + 160) * 4..][..3];
            assert!(px[0] > 200 && px[1] < 60, "red, got {px:?}");
        }
        // Past the item time the broken second item is skipped, back to
        // the clip.
        std::thread::sleep(Duration::from_millis(350));
        assert!(p.next(320, 240).is_ok());
        assert_eq!(p.media.as_ref().unwrap().index, 0);
        std::fs::remove_dir_all(&dir).unwrap();
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
