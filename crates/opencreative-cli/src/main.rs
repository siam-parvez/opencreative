//! `opencreative` command-line tool.

mod uninstall;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use opencreative_config::{FitConfig, MediaItem, Metric, Rgb, SceneConfig, SkinConfig};
use opencreative_core::{Player, Scene};
use opencreative_device as device;
use opencreative_ipc::{self as ipc, Request, Response};
use opencreative_render::media::{self, Fit};
use opencreative_render::panels::Layout;
use opencreative_render::{Rotation, monitor, rotate};
use opencreative_sensors::{ClaudeStatus, Sensors};

#[derive(Parser)]
#[command(
    name = "opencreative",
    version,
    about = "Open-source control for DeepCool LCD coolers"
)]
struct Cli {
    /// Pick a device by USB serial number (see `opencreative list`).
    #[arg(long, global = true)]
    serial: Option<String>,

    /// Rotate screen content clockwise to match how the pump is mounted.
    #[arg(long, global = true, value_parser = parse_rotation)]
    rotate: Option<u16>,

    /// Use the device directly even if the daemon is running.
    #[arg(long, global = true)]
    no_daemon: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected DeepCool displays.
    List,
    /// Print sensor readings and every temperature sensor found.
    Sensors,
    /// Set backlight brightness (0-100).
    Brightness {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: u8,
    },
    /// Show a calm screen with a dim clock (Zen mode), or go back.
    Zen {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Turn the screen off (backlight off) or back on.
    Screen {
        #[arg(value_enum)]
        state: OnOff,
    },
    /// Fill the screen with one color, e.g. `#ff8800`.
    Solid { color: Rgb },
    /// Show images, GIFs or videos (files or web addresses, e.g. YouTube),
    /// one after another. Videos need ffmpeg; web pages need yt-dlp.
    #[command(visible_alias = "image")]
    Media {
        /// Files or http(s) addresses, played in order.
        #[arg(required = true)]
        sources: Vec<String>,
        #[arg(long, value_enum, default_value_t = FitArg::Cover)]
        fit: FitArg,
        /// Zoom in: 1 shows everything, 2 half the width and height.
        #[arg(long, default_value_t = 1.0)]
        zoom: f32,
        /// The point to zoom in on, as fractions: `0.5,0.5` is the middle.
        #[arg(long, value_parser = parse_focus, default_value = "0.5,0.5")]
        focus: [f32; 2],
        /// Seconds per item when there are several.
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
    },
    /// Show chosen readings in outlined panels: one large, two small.
    Panels {
        /// Look of the screen.
        #[arg(long, value_enum, default_value_t = SkinArg::Lines)]
        skin: SkinArg,
        /// The large reading.
        #[arg(long, default_value = "cpu-temp")]
        primary: Metric,
        /// The two small readings (give the flag twice).
        #[arg(long, num_args = 1, default_values = ["cpu-freq", "cpu-load"])]
        secondary: Vec<Metric>,
        /// Seconds between updates.
        #[arg(long, default_value_t = 1.0)]
        interval: f64,
    },
    /// Show live CPU/GPU temperatures, load, RAM and the clock as gauges.
    Monitor {
        /// Seconds between updates.
        #[arg(long, default_value_t = 1.0)]
        interval: f64,
    },
    /// Render a screen to a PNG without touching the device.
    Preview {
        #[command(subcommand)]
        what: PreviewWhat,
        /// Output PNG path.
        #[arg(short, long, global = true, default_value = "preview.png")]
        output: PathBuf,
    },
    /// Show what the background daemon is doing.
    Status,
    /// Run in the background, owning the device. Other commands are sent to
    /// it while it runs, and its settings persist in the config file.
    Daemon {
        /// Config file (default: the user's config directory).
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Remove OpenCreative installed from a download (not a package): the
    /// program, background service, app-menu entry and udev rules.
    Uninstall {
        /// Also remove your settings.
        #[arg(long)]
        purge: bool,
        /// Don't ask for confirmation.
        #[arg(long, short)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum PreviewWhat {
    Monitor,
    Image {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = FitArg::Cover)]
        fit: FitArg,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum SkinArg {
    Lines,
    Hud,
}

impl From<SkinArg> for SkinConfig {
    fn from(s: SkinArg) -> Self {
        match s {
            SkinArg::Lines => SkinConfig::Lines,
            SkinArg::Hud => SkinConfig::Hud,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum FitArg {
    Cover,
    Contain,
    Stretch,
}

impl From<FitArg> for FitConfig {
    fn from(f: FitArg) -> Self {
        match f {
            FitArg::Cover => FitConfig::Cover,
            FitArg::Contain => FitConfig::Contain,
            FitArg::Stretch => FitConfig::Stretch,
        }
    }
}

impl From<FitArg> for Fit {
    fn from(f: FitArg) -> Self {
        FitConfig::from(f).into()
    }
}

fn parse_rotation(s: &str) -> Result<u16, String> {
    let deg: u16 = s.parse().map_err(|_| format!("not a number: {s}"))?;
    Rotation::from_degrees(deg)
        .map(|_| deg)
        .ok_or_else(|| "rotation must be 0, 90, 180 or 270".to_owned())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let rotation = cli
        .rotate
        .map(|deg| Rotation::from_degrees(deg).expect("validated by clap"));
    let serial = cli.serial.as_deref();

    match cli.command {
        Command::List => list(),
        Command::Sensors => sensors(),
        Command::Preview { what, output } => preview(what, &output, rotation.unwrap_or_default()),
        Command::Daemon { config } => opencreative_daemon::run(config),
        Command::Uninstall { purge, yes } => uninstall::run(purge, yes),
        Command::Status => status(connect_daemon(cli.no_daemon)?),
        command => match connect_daemon(cli.no_daemon)? {
            Some(client) => send_to_daemon(client, command, cli.rotate, serial),
            None => run_direct(command, serial, rotation.unwrap_or_default()),
        },
    }
}

fn connect_daemon(disabled: bool) -> Result<Option<ipc::Client>> {
    if disabled {
        return Ok(None);
    }
    Ok(ipc::Client::connect(&ipc::default_socket_path())?)
}

/// Runs a screen command against the device directly.
fn run_direct(command: Command, serial: Option<&str>, rotation: Rotation) -> Result<()> {
    match command {
        Command::Brightness { percent } => Ok(device::open(serial)?.set_brightness(percent)?),
        Command::Zen { state } => match state {
            OnOff::On => play(Scene::Zen, serial, rotation),
            OnOff::Off => bail!("Zen mode runs until interrupted when there is no daemon"),
        },
        Command::Screen { state } => {
            // Without a daemon there is no remembered level; use the saved one.
            let level = match state {
                OnOff::Off => 0,
                OnOff::On => saved_brightness().unwrap_or(100),
            };
            Ok(device::open(serial)?.set_brightness(level)?)
        }
        Command::Solid { color } => play(Scene::Solid(color.0), serial, rotation),
        media @ Command::Media { .. } => {
            let scene = media_scene(&media)?;
            play(scene.to_scene()?, serial, rotation)
        }
        Command::Monitor { interval } => {
            let interval = interval_from_secs(interval)?;
            play(Scene::Monitor { interval }, serial, rotation)
        }
        Command::Panels {
            skin,
            primary,
            secondary,
            interval,
        } => {
            let layout = Layout {
                primary,
                secondary: two_secondaries(&secondary)?,
            };
            let interval = interval_from_secs(interval)?;
            let skin = SkinConfig::from(skin).into();
            let scene = Scene::Panels {
                layout,
                skin,
                interval,
            };
            play(scene, serial, rotation)
        }
        _ => unreachable!("not a screen command"),
    }
}

/// Hands a screen command to the running daemon, which owns the device.
fn send_to_daemon(
    mut client: ipc::Client,
    command: Command,
    rotate: Option<u16>,
    serial: Option<&str>,
) -> Result<()> {
    if serial.is_some() {
        log::warn!("--serial is ignored while the daemon runs; set `serial` in its config");
    }
    let mut requests = Vec::new();
    if let Some(degrees) = rotate {
        requests.push(Request::SetRotation { degrees });
    }
    requests.push(match command {
        Command::Brightness { percent } => Request::SetBrightness { percent },
        Command::Zen { state } => Request::SetZen {
            on: matches!(state, OnOff::On),
        },
        Command::Screen { state } => Request::SetScreen {
            on: matches!(state, OnOff::On),
        },
        Command::Solid { color } => Request::SetScene {
            scene: SceneConfig::Solid { color },
        },
        media @ Command::Media { .. } => Request::SetScene {
            scene: media_scene(&media)?,
        },
        Command::Panels {
            skin,
            primary,
            secondary,
            interval,
        } => Request::SetScene {
            scene: SceneConfig::Panels {
                skin: skin.into(),
                primary,
                secondary: two_secondaries(&secondary)?,
                interval_secs: interval,
            },
        },
        Command::Monitor { interval } => Request::SetScene {
            scene: SceneConfig::Monitor {
                interval_secs: interval,
            },
        },
        _ => unreachable!("not a screen command"),
    });
    for request in &requests {
        match client.request(request)? {
            Response::Ok => {}
            Response::Error { message } => bail!("daemon: {message}"),
            other => bail!("unexpected daemon response: {other:?}"),
        }
    }
    println!("Sent to the OpenCreative daemon.");
    Ok(())
}

fn interval_from_secs(secs: f64) -> Result<Duration> {
    if !(secs.is_finite() && secs > 0.0) {
        bail!("interval must be a positive number of seconds");
    }
    Ok(Duration::from_secs_f64(secs))
}

fn two_secondaries(given: &[Metric]) -> Result<[Metric; 2]> {
    given
        .try_into()
        .map_err(|_| anyhow::anyhow!("give exactly two --secondary readings"))
}

/// Brightness from the config file, if one sets it.
fn saved_brightness() -> Option<u8> {
    let path = opencreative_config::default_path().ok()?;
    opencreative_config::load(&path).ok()?.brightness
}

fn status(client: Option<ipc::Client>) -> Result<()> {
    let Some(mut client) = client else {
        println!(
            "No daemon running (socket {}).",
            ipc::default_socket_path().display()
        );
        return Ok(());
    };
    match client.request(&Request::Status)? {
        Response::Status(status) => {
            println!(
                "Device: {}",
                status.device.as_deref().unwrap_or("not connected")
            );
            println!("Screen: {}", if status.screen_on { "on" } else { "off" });
            println!("Zen mode: {}", if status.zen { "on" } else { "off" });
            print!("{}", toml::to_string_pretty(&status.config)?);
            Ok(())
        }
        other => bail!("unexpected daemon response: {other:?}"),
    }
}

fn list() -> Result<()> {
    let found = device::discover()?;
    if found.is_empty() {
        println!("No supported DeepCool display found.");
        println!("Supported models:");
        for spec in device::SUPPORTED {
            println!(
                "  {} ({:04x}:{:04x})",
                spec.name, spec.vendor_id, spec.product_id
            );
        }
    }
    for f in found {
        println!(
            "{}  {}x{}  bus {} addr {:03}  serial {}",
            f.spec.name,
            f.spec.width,
            f.spec.height,
            f.bus(),
            f.address(),
            f.serial().unwrap_or("-"),
        );
    }
    Ok(())
}

fn sensors() -> Result<()> {
    let mut sensors = Sensors::new();
    opencreative_sensors::set_claude_enabled(true);
    sensors.read();
    thread::sleep(Duration::from_millis(500));
    // Give the first Claude usage request a moment to finish.
    for _ in 0..20 {
        if opencreative_sensors::claude_status() != ClaudeStatus::Off {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let snap = sensors.read();
    println!("{snap:#?}");
    println!(
        "\nClaude usage: {:?}",
        opencreative_sensors::claude_status()
    );
    println!("\nTemperature sensors:");
    for (label, temp) in sensors.all_temperatures() {
        match temp {
            Some(t) => println!("  {label:<40} {t:5.1} °C"),
            None => println!("  {label:<40}     -"),
        }
    }
    Ok(())
}

/// The playlist a `media` command asks for, with files made absolute (the
/// daemon has its own working directory) and checked to exist.
fn media_scene(command: &Command) -> Result<SceneConfig> {
    let Command::Media {
        sources,
        fit,
        zoom,
        focus,
        seconds,
    } = command
    else {
        unreachable!("only called for `media`");
    };
    let items = sources
        .iter()
        .map(|source| {
            let mut item = if source.starts_with("http://") || source.starts_with("https://") {
                MediaItem::web(source.clone())
            } else {
                let path = fs::canonicalize(source).with_context(|| format!("opening {source}"))?;
                MediaItem::file(path)
            };
            item.fit = (*fit).into();
            item.zoom = *zoom;
            item.focus = *focus;
            Ok(item)
        })
        .collect::<Result<Vec<_>>>()?;
    let scene = SceneConfig::Media {
        items,
        item_secs: *seconds,
    };
    scene.to_scene()?;
    Ok(scene)
}

fn parse_focus(s: &str) -> Result<[f32; 2], String> {
    let (x, y) = s.split_once(',').ok_or("expected x,y like 0.5,0.5")?;
    let parse = |v: &str| {
        v.trim()
            .parse::<f32>()
            .ok()
            .filter(|v| (0.0..=1.0).contains(v))
            .ok_or_else(|| format!("{v} is not a fraction between 0 and 1"))
    };
    Ok([parse(x)?, parse(y)?])
}

/// Render size for the first supported model, used when no device is open.
fn default_logical_size(rotation: Rotation) -> (u32, u32) {
    let spec = &device::SUPPORTED[0];
    rotation.logical_size(spec.width, spec.height)
}

/// Shows a scene. Animated scenes and the monitor run until interrupted.
fn play(scene: Scene, serial: Option<&str>, rotation: Rotation) -> Result<()> {
    let mut player = Player::new(scene, rotation);
    opencreative_core::run(&mut player, serial)?;
    Ok(())
}

fn preview(what: PreviewWhat, output: &Path, rotation: Rotation) -> Result<()> {
    let (w, h) = default_logical_size(rotation);
    let frame = match what {
        PreviewWhat::Monitor => {
            let mut sensors = Sensors::new();
            sensors.read();
            thread::sleep(Duration::from_millis(500));
            let clock = chrono::Local::now().format("%H:%M").to_string();
            monitor::render(&sensors.read(), &clock, w, h)
        }
        PreviewWhat::Image { path, fit } => {
            media::load(&path, w, h, fit.into(), media::Crop::default())
                .with_context(|| format!("loading {}", path.display()))?
                .into_iter()
                .next()
                .context("image has no frames")?
                .pixmap
        }
    };
    // Save in panel orientation, exactly what the device would receive.
    rotate(&frame, rotation)
        .save_png(output)
        .with_context(|| format!("writing {}", output.display()))?;
    println!("wrote {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations() {
        assert_eq!(parse_rotation("90"), Ok(90));
        assert!(parse_rotation("45").is_err());
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
