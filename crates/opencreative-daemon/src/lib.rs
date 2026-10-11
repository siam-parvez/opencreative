//! The OpenCreative daemon: owns the device, serves IPC requests, and
//! persists every change to the config file so it survives restarts.
//!
//! Run by `opencreative daemon`, and in-process by the GUI when no daemon is
//! running.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use opencreative_config::{self as config, Config};
use opencreative_core::{Command, DaemonState, Player, run_daemon};
use opencreative_ipc::{self as ipc, Request, Response, Status};
use opencreative_render::Rotation;

/// Runs until the IPC listener fails. Errors early if another daemon is
/// already running.
pub fn run(config_path: Option<PathBuf>) -> Result<()> {
    let config_path = match config_path {
        Some(path) => path,
        None => config::default_path()?,
    };
    let config = config::load(&config_path)?;
    let socket = ipc::default_socket_path();
    // Bind first: if another daemon is running, stop before touching the device.
    let server = ipc::Server::bind(&socket)?;

    let player = Player::new(config.scene.to_scene()?, config.rotation()?);
    let (commands, receiver) = mpsc::channel();
    let state = Arc::new(Mutex::new(DaemonState::default()));
    {
        let (serial, brightness, state) = (config.serial.clone(), config.brightness, state.clone());
        thread::Builder::new()
            .name("playback".into())
            .spawn(move || run_daemon(player, serial, brightness, receiver, state))
            .context("starting playback thread")?;
    }

    log::info!(
        "daemon listening on {} (config {})",
        socket.display(),
        config_path.display()
    );
    let shared = Shared {
        config: Mutex::new(config),
        config_path,
        commands,
        state,
    };
    server.serve(move |request| shared.handle(request))?;
    Ok(())
}

struct Shared {
    config: Mutex<Config>,
    config_path: PathBuf,
    commands: Sender<Command>,
    state: Arc<Mutex<DaemonState>>,
}

impl Shared {
    fn handle(&self, request: Request) -> Response {
        let mut config = self.config.lock().expect("config lock");
        let command = match request {
            Request::Status => {
                let state = self.state.lock().expect("state lock");
                return Response::Status(Status {
                    device: state.device.clone(),
                    screen_on: state.screen_on,
                    config: config.clone(),
                });
            }
            Request::SetScene { scene } => match scene.to_scene() {
                Ok(parsed) => {
                    config.scene = scene;
                    Command::SetScene(parsed)
                }
                Err(e) => return error(e),
            },
            Request::SetBrightness { percent } if percent <= 100 => {
                config.brightness = Some(percent);
                Command::SetBrightness(percent)
            }
            Request::SetBrightness { percent } => {
                return error(format!("brightness must be 0-100, not {percent}"));
            }
            Request::SetRotation { degrees } => match Rotation::from_degrees(degrees) {
                Some(rotation) => {
                    config.rotation = degrees;
                    Command::SetRotation(rotation)
                }
                None => return error(format!("rotation must be 0, 90, 180 or 270, not {degrees}")),
            },
            // Session state, not a setting: the screen is on after a restart.
            Request::SetScreen { on } => Command::SetScreen(on),
            Request::Hello { .. } => return error("hello is handled by the server"),
        };
        save(&self.config_path, &config);
        match self.commands.send(command) {
            Ok(()) => Response::Ok,
            Err(_) => error("playback thread has stopped"),
        }
    }
}

fn save(path: &Path, config: &Config) {
    if let Err(e) = config::save(path, config) {
        log::warn!("couldn't save config: {e}");
    }
}

fn error(message: impl ToString) -> Response {
    Response::Error {
        message: message.to_string(),
    }
}
