//! The OpenCreative daemon: owns the device, serves IPC requests, and
//! persists every change to the config file so it survives restarts.
//!
//! Run by `opencreative daemon`, and in-process by the GUI when no daemon is
//! running.

mod tray;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use opencreative_config::{self as config, Config};
use opencreative_core::{Command, DaemonState, Player, run_daemon};
use opencreative_ipc::{self as ipc, Request, Response, Status};
use opencreative_render::Rotation;

/// Runs the daemon on the calling thread until it stops (normally never),
/// with its top-bar icon. Errors early if another daemon is already running.
pub fn run(config_path: Option<PathBuf>) -> Result<()> {
    spawn(config_path, true)?.wait()
}

/// A daemon running on background threads.
pub struct Daemon {
    stopper: ipc::Stopper,
    commands: Sender<Command>,
    server: JoinHandle<Result<()>>,
    playback: JoinHandle<()>,
}

impl Daemon {
    /// Blocks until the daemon stops.
    pub fn wait(self) -> Result<()> {
        join(self.server)?
    }

    /// Stops serving, releases the device and removes the socket, so another
    /// daemon (e.g. the system service) can take over.
    pub fn stop(self) -> Result<()> {
        self.stopper.stop();
        let _ = self.commands.send(Command::Shutdown);
        join(self.playback)?;
        join(self.server)?
    }
}

fn join<T>(handle: JoinHandle<T>) -> Result<T> {
    handle.join().map_err(|_| anyhow!("daemon thread panicked"))
}

/// Starts the daemon on background threads. The socket is bound before this
/// returns, so "already running" and config errors surface here. With
/// `tray`, it shows a top-bar icon unless the config hides it.
pub fn spawn(config_path: Option<PathBuf>, tray: bool) -> Result<Daemon> {
    let config_path = match config_path {
        Some(path) => path,
        None => config::default_path()?,
    };
    let config = config::load(&config_path)?;
    let socket = ipc::default_socket_path();
    // Bind first: if another daemon is running, stop before touching the device.
    let server = ipc::Server::bind(&socket)?;
    let stopper = server.stopper();

    let player = Player::new(config.scene.to_scene()?, config.rotation()?);
    let (commands, receiver) = mpsc::channel();
    let state = Arc::new(Mutex::new(DaemonState::default()));
    let playback = {
        let (serial, brightness, state) = (config.serial.clone(), config.brightness, state.clone());
        thread::Builder::new()
            .name("playback".into())
            .spawn(move || run_daemon(player, serial, brightness, receiver, state))
            .context("starting playback thread")?
    };

    log::info!(
        "daemon listening on {} (config {})",
        socket.display(),
        config_path.display()
    );
    let show_tray = tray && !config.hide_tray;
    let shared = Arc::new_cyclic(|me| Shared {
        config: Mutex::new(config),
        config_path,
        commands: commands.clone(),
        state,
        me: me.clone(),
        tray_allowed: tray,
        tray: Mutex::new(None),
    });
    if tray {
        shared.set_tray(show_tray);
        // Keeps the icon's menu and tooltip current as coolers come and go.
        let watcher = Arc::downgrade(&shared);
        thread::Builder::new()
            .name("tray-refresh".into())
            .spawn(move || {
                while let Some(shared) = watcher.upgrade() {
                    shared.refresh_tray();
                    drop(shared);
                    thread::sleep(Duration::from_secs(1));
                }
            })
            .context("starting tray thread")?;
    }
    let server = thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            Ok(server.serve(move |request| {
                let changes = !matches!(
                    request,
                    Request::Status | Request::Frame { .. } | Request::Hello { .. }
                );
                let response = shared.handle(request);
                if changes {
                    shared.refresh_tray();
                }
                response
            })?)
        })
        .context("starting IPC thread")?;
    Ok(Daemon {
        stopper,
        commands,
        server,
        playback,
    })
}

struct Shared {
    config: Mutex<Config>,
    config_path: PathBuf,
    commands: Sender<Command>,
    state: Arc<Mutex<DaemonState>>,
    /// This, for starting the tray from a request.
    me: Weak<Shared>,
    /// Whether this daemon may show a top-bar icon at all.
    tray_allowed: bool,
    tray: Mutex<Option<ksni::blocking::Handle<tray::CoolerTray>>>,
}

impl Shared {
    /// What the top-bar icon's menu shows.
    fn view(&self) -> tray::View {
        let brightness = self.config.lock().expect("config lock").brightness;
        let state = self.state.lock().expect("state lock");
        tray::View {
            device: state.device.clone(),
            screen_on: state.screen_on,
            zen: state.zen,
            brightness: brightness.unwrap_or(100),
        }
    }

    /// Shows or hides the top-bar icon.
    fn set_tray(&self, on: bool) {
        if !self.tray_allowed {
            return;
        }
        let mut slot = self.tray.lock().expect("tray lock");
        match (on, slot.is_some()) {
            (true, false) => {
                if let Some(me) = self.me.upgrade() {
                    // Starting it reads the view, so not under the lock.
                    drop(slot);
                    let handle = tray::show(me);
                    *self.tray.lock().expect("tray lock") = handle;
                }
            }
            (false, true) => {
                if let Some(handle) = slot.take() {
                    // Not waited on: this may run on the tray's own thread.
                    drop(handle.shutdown());
                }
            }
            _ => {}
        }
    }

    /// Updates the icon's menu after a change made elsewhere. Never called
    /// from the tray's own thread, which updates itself.
    fn refresh_tray(&self) {
        let handle = self.tray.lock().expect("tray lock").clone();
        if let Some(handle) = handle {
            tray::refresh(&handle, self.view());
        }
    }

    fn handle(&self, request: Request) -> Response {
        let mut config = self.config.lock().expect("config lock");
        let command = match request {
            Request::Status => {
                let state = self.state.lock().expect("state lock");
                return Response::Status(Status {
                    device: state.device.clone(),
                    screen_on: state.screen_on,
                    zen: state.zen,
                    config: config.clone(),
                });
            }
            Request::Frame { after } => {
                let frame = self.state.lock().expect("state lock").frame.clone();
                return Response::Frame {
                    frame: frame.filter(|f| f.seq != after).map(|f| {
                        let rgb565 = opencreative_protocol::pixel::rgba_to_rgb565_le(
                            &f.rgba,
                            f.width as usize,
                        );
                        ipc::Frame::new(f.seq, f.width, f.height, f.rotation.degrees(), &rgb565)
                    }),
                };
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
            Request::SetZen { on } => Command::SetZen(on),
            Request::SetTray { on } => {
                config.hide_tray = !on;
                save(&self.config_path, &config);
                drop(config);
                self.set_tray(on);
                return Response::Ok;
            }
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
