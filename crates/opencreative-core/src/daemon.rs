//! Long-running playback that takes commands while it runs.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opencreative_device::{self as device, Display};
use opencreative_render::Rotation;

use crate::{Player, RECONNECT_DELAY, Scene};

/// A change requested while the daemon runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetScene(Scene),
    SetRotation(Rotation),
    /// Draws readings, music bars and Zen dark or light.
    SetShade(opencreative_render::Shade),
    SetBrightness(u8),
    /// Turns the screen on or off. Off is backlight 0 with frames paused:
    /// the protocol has no confirmed "display off" command.
    SetScreen(bool),
    /// Shows the calm Zen screen instead of the chosen one, or goes back.
    SetZen(bool),
    /// Stops playback and releases the device.
    Shutdown,
}

/// What the daemon reports about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct DaemonState {
    /// Name of the connected device, if any.
    pub device: Option<String>,
    pub screen_on: bool,
    pub zen: bool,
    /// The frame on the screen now, for previews; `None` while nothing is
    /// shown (no device, or the screen is off).
    pub frame: Option<SentFrame>,
}

/// A frame as sent to the device: panel orientation, RGBA.
#[derive(Debug, Clone, PartialEq)]
pub struct SentFrame {
    /// Counts up with every frame sent.
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    /// How the content was rotated to fit the panel.
    pub rotation: Rotation,
    pub rgba: Arc<Vec<u8>>,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            device: None,
            screen_on: true,
            zen: false,
            frame: None,
        }
    }
}

/// How often a still frame is re-sent, so it comes back after a replug.
const STILL_REFRESH: Duration = Duration::from_secs(5);

/// Brightness used when turning the screen back on if none was ever chosen.
const DEFAULT_BRIGHTNESS: u8 = 100;

/// Keeps `player`'s scene on the device until [`Command::Shutdown`] arrives or
/// `commands` is closed. Waits for
/// a device to appear, reconnects when it disappears, and applies commands as
/// soon as they arrive. Never gives up on errors: a background service should
/// recover once the cause (a missing udev rule, an unplugged cooler, a deleted
/// image) is fixed.
pub fn run_daemon(
    mut player: Player,
    serial: Option<String>,
    mut brightness: Option<u8>,
    commands: Receiver<Command>,
    state: Arc<Mutex<DaemonState>>,
) {
    let mut dev: Option<Box<dyn Display>> = None;
    let mut screen_on = true;
    // While Zen mode is on, this plays instead of `player`.
    let mut zen: Option<Player> = None;
    let update = |f: &dyn Fn(&mut DaemonState)| f(&mut state.lock().expect("state lock"));

    loop {
        if dev.is_none() {
            match device::open(serial.as_deref()) {
                Ok(mut d) => {
                    log::info!("connected to {}", d.spec().name);
                    let level = if screen_on { brightness } else { Some(0) };
                    if let Some(level) = level {
                        set_backlight(d.as_mut(), level);
                    }
                    let name = d.spec().name;
                    update(&|s| s.device = Some(name.to_owned()));
                    dev = Some(d);
                }
                Err(e) => log::warn!("{e}; retrying in {RECONNECT_DELAY:?}"),
            }
        }

        let wait = match dev.as_mut() {
            None => RECONNECT_DELAY,
            // Dark screen: nothing to draw until it is turned back on.
            Some(_) if !screen_on => {
                update(&|s| s.frame = None);
                STILL_REFRESH
            }
            Some(d) => {
                let started = Instant::now();
                let spec = d.spec();
                let active = zen.as_mut().unwrap_or(&mut player);
                match active.next(spec.width, spec.height) {
                    Ok(out) => match d.send_frame(out.frame.data()) {
                        Ok(()) => {
                            let rgba = Arc::new(out.frame.data().to_vec());
                            let rotation = active.rotation();
                            update(&|s| {
                                s.frame = Some(SentFrame {
                                    seq: s.frame.as_ref().map_or(1, |f| f.seq + 1),
                                    width: spec.width,
                                    height: spec.height,
                                    rotation,
                                    rgba: Arc::clone(&rgba),
                                });
                            });
                            out.hold
                                .unwrap_or(STILL_REFRESH)
                                .saturating_sub(started.elapsed())
                        }
                        Err(e) => {
                            log::warn!("lost device: {e}");
                            dev = None;
                            update(&|s| {
                                s.device = None;
                                s.frame = None;
                            });
                            RECONNECT_DELAY
                        }
                    },
                    Err(e) => {
                        log::error!("can't show scene: {e}");
                        STILL_REFRESH
                    }
                }
            }
        };

        // Sleep until the next frame is due, but act on commands right away.
        let deadline = Instant::now() + wait;
        loop {
            let command =
                match commands.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(command) => command,
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
            log::info!("command: {command:?}");
            match command {
                Command::Shutdown => return,
                Command::SetScene(scene) => {
                    player = Player::new(scene, player.rotation()).with_shade(player.shade());
                    break;
                }
                Command::SetRotation(rotation) => {
                    player =
                        Player::new(player.scene().clone(), rotation).with_shade(player.shade());
                    if zen.is_some() {
                        zen = Some(Player::new(Scene::Zen, rotation).with_shade(player.shade()));
                    }
                    break;
                }
                Command::SetShade(shade) => {
                    player =
                        Player::new(player.scene().clone(), player.rotation()).with_shade(shade);
                    if zen.is_some() {
                        zen = Some(Player::new(Scene::Zen, player.rotation()).with_shade(shade));
                    }
                    break;
                }
                Command::SetZen(on) => {
                    zen = on.then(|| {
                        Player::new(Scene::Zen, player.rotation()).with_shade(player.shade())
                    });
                    update(&|s| s.zen = on);
                    break;
                }
                Command::SetBrightness(percent) => {
                    brightness = Some(percent);
                    // While off, remember it for when the screen comes back.
                    if screen_on && let Some(d) = dev.as_mut() {
                        set_backlight(d.as_mut(), percent);
                    }
                }
                Command::SetScreen(on) => {
                    screen_on = on;
                    update(&|s| s.screen_on = on);
                    if let Some(d) = dev.as_mut() {
                        let level = if on {
                            brightness.unwrap_or(DEFAULT_BRIGHTNESS)
                        } else {
                            0
                        };
                        set_backlight(d.as_mut(), level);
                    }
                    if on {
                        break;
                    }
                }
            }
        }
    }
}

fn set_backlight(dev: &mut dyn Display, percent: u8) {
    if let Err(e) = dev.set_brightness(percent) {
        log::warn!("setting brightness: {e}");
    }
}
