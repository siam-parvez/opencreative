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
    SetBrightness(u8),
    ScreenToggle,
}

/// What the daemon reports about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DaemonState {
    /// Name of the connected device, if any.
    pub device: Option<String>,
}

/// How often a still frame is re-sent, so it comes back after a replug.
const STILL_REFRESH: Duration = Duration::from_secs(5);

/// Keeps `player`'s scene on the device until `commands` is closed. Waits for
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
    let set_device = |name: Option<&str>| {
        state.lock().expect("state lock").device = name.map(str::to_owned);
    };

    loop {
        if dev.is_none() {
            match device::open(serial.as_deref()) {
                Ok(mut d) => {
                    log::info!("connected to {}", d.spec().name);
                    if let Some(b) = brightness
                        && let Err(e) = d.set_brightness(b)
                    {
                        log::warn!("setting brightness: {e}");
                    }
                    set_device(Some(d.spec().name));
                    dev = Some(d);
                }
                Err(e) => log::warn!("{e}; retrying in {RECONNECT_DELAY:?}"),
            }
        }

        let wait = match dev.as_mut() {
            None => RECONNECT_DELAY,
            Some(d) => {
                let started = Instant::now();
                let spec = d.spec();
                match player.next(spec.width, spec.height) {
                    Ok(out) => match d.send_frame(out.frame.data()) {
                        Ok(()) => out
                            .hold
                            .unwrap_or(STILL_REFRESH)
                            .saturating_sub(started.elapsed()),
                        Err(e) => {
                            log::warn!("lost device: {e}");
                            dev = None;
                            set_device(None);
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
                Command::SetScene(scene) => {
                    player = Player::new(scene, player.rotation());
                    break;
                }
                Command::SetRotation(rotation) => {
                    player = Player::new(player.scene().clone(), rotation);
                    break;
                }
                Command::SetBrightness(percent) => {
                    brightness = Some(percent);
                    if let Some(d) = dev.as_mut()
                        && let Err(e) = d.set_brightness(percent)
                    {
                        log::warn!("setting brightness: {e}");
                    }
                }
                Command::ScreenToggle => {
                    if let Some(d) = dev.as_mut()
                        && let Err(e) = d.screen_toggle()
                    {
                        log::warn!("toggling screen: {e}");
                    }
                }
            }
        }
    }
}
