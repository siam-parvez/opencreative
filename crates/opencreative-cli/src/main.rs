//! `opencreative` command-line tool.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use opencreative_device::{self as device, Display};
use opencreative_render::media::{self, Fit};
use opencreative_render::{Pixmap, Rotation, monitor, rotate, solid};
use opencreative_sensors::Sensors;

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
    #[arg(long, global = true, default_value_t = 0, value_parser = parse_rotation)]
    rotate: u16,

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
    /// Toggle the screen on/off.
    ScreenToggle,
    /// Fill the screen with one color, e.g. `#ff8800`.
    Solid {
        #[arg(value_parser = parse_hex_color)]
        color: [u8; 3],
    },
    /// Show an image. Animated GIFs loop until interrupted.
    Image {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = FitArg::Cover)]
        fit: FitArg,
    },
    /// Show live CPU/GPU temperatures, load, RAM and the clock.
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
enum FitArg {
    Cover,
    Contain,
    Stretch,
}

impl From<FitArg> for Fit {
    fn from(f: FitArg) -> Self {
        match f {
            FitArg::Cover => Fit::Cover,
            FitArg::Contain => Fit::Contain,
            FitArg::Stretch => Fit::Stretch,
        }
    }
}

fn parse_rotation(s: &str) -> Result<u16, String> {
    let deg: u16 = s.parse().map_err(|_| format!("not a number: {s}"))?;
    Rotation::from_degrees(deg)
        .map(|_| deg)
        .ok_or_else(|| "rotation must be 0, 90, 180 or 270".to_owned())
}

fn parse_hex_color(s: &str) -> Result<[u8; 3], String> {
    let hex = s.trim_start_matches('#');
    if hex.len() != 6 {
        return Err("expected a color like #ff8800".to_owned());
    }
    let v = u32::from_str_radix(hex, 16).map_err(|_| format!("invalid hex color: {s}"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let rotation = Rotation::from_degrees(cli.rotate).expect("validated by clap");
    let serial = cli.serial.as_deref();

    match cli.command {
        Command::List => list(),
        Command::Sensors => sensors(),
        Command::Brightness { percent } => Ok(device::open(serial)?.set_brightness(percent)?),
        Command::ScreenToggle => Ok(device::open(serial)?.screen_toggle()?),
        Command::Solid { color } => {
            let mut dev = device::open(serial)?;
            let (w, h) = logical_size(dev.as_ref(), rotation);
            show(dev.as_mut(), &solid(w, h, color), rotation)
        }
        Command::Image { path, fit } => image(serial, &path, fit.into(), rotation),
        Command::Monitor { interval } => run_monitor(serial, interval, rotation),
        Command::Preview { what, output } => preview(what, &output, rotation),
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
            "{}  {}x{}  bus {:03} addr {:03}  serial {}",
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
    sensors.read();
    thread::sleep(Duration::from_millis(500));
    let snap = sensors.read();
    println!("{snap:#?}");
    println!("\nTemperature sensors:");
    for (label, temp) in sensors.all_temperatures() {
        match temp {
            Some(t) => println!("  {label:<40} {t:5.1} °C"),
            None => println!("  {label:<40}     -"),
        }
    }
    Ok(())
}

/// Render size for the first supported model, used when no device is open.
fn default_logical_size(rotation: Rotation) -> (u32, u32) {
    let spec = &device::SUPPORTED[0];
    rotation.logical_size(spec.width, spec.height)
}

fn logical_size(dev: &dyn Display, rotation: Rotation) -> (u32, u32) {
    let spec = dev.spec();
    rotation.logical_size(spec.width, spec.height)
}

fn show(dev: &mut dyn Display, frame: &Pixmap, rotation: Rotation) -> Result<()> {
    dev.send_frame(rotate(frame, rotation).data())?;
    Ok(())
}

fn image(serial: Option<&str>, path: &Path, fit: Fit, rotation: Rotation) -> Result<()> {
    let mut dev = device::open(serial)?;
    let (w, h) = logical_size(dev.as_ref(), rotation);
    let frames =
        media::load(path, w, h, fit).with_context(|| format!("loading {}", path.display()))?;
    // Rotate once up front so animation playback is just USB writes.
    let frames: Vec<_> = frames
        .into_iter()
        .map(|f| (rotate(&f.pixmap, rotation), f.delay))
        .collect();

    if let [(still, _)] = frames.as_slice() {
        dev.send_frame(still.data())?;
        return Ok(());
    }
    log::info!("playing {} frames, Ctrl+C to stop", frames.len());
    loop {
        for (frame, delay) in &frames {
            let started = Instant::now();
            dev.send_frame(frame.data())?;
            thread::sleep(delay.saturating_sub(started.elapsed()));
        }
    }
}

/// How long to wait before retrying after the device disappears (e.g. a
/// replug or suspend/resume).
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

fn run_monitor(serial: Option<&str>, interval: f64, rotation: Rotation) -> Result<()> {
    if !(interval.is_finite() && interval > 0.0) {
        bail!("interval must be a positive number of seconds");
    }
    let interval = Duration::from_secs_f64(interval);
    let mut sensors = Sensors::new();
    let mut dev: Option<Box<dyn Display>> = None;

    loop {
        let started = Instant::now();
        let snap = sensors.read();

        if dev.is_none() {
            match device::open(serial) {
                Ok(d) => {
                    log::info!("connected to {}", d.spec().name);
                    dev = Some(d);
                }
                Err(e @ device::Error::PermissionDenied) => return Err(e.into()),
                Err(e) => {
                    log::warn!("{e}; retrying in {RECONNECT_DELAY:?}");
                    thread::sleep(RECONNECT_DELAY);
                    continue;
                }
            }
        }
        let d = dev.as_mut().expect("connected above");
        let (w, h) = logical_size(d.as_ref(), rotation);
        let clock = chrono::Local::now().format("%H:%M").to_string();
        let frame = monitor::render(&snap, &clock, w, h);
        if let Err(e) = show(d.as_mut(), &frame, rotation) {
            log::warn!("lost device: {e}");
            dev = None;
        }
        thread::sleep(interval.saturating_sub(started.elapsed()));
    }
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
            media::load(&path, w, h, fit.into())
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
    fn hex_colors() {
        assert_eq!(parse_hex_color("#ff8800"), Ok([0xff, 0x88, 0x00]));
        assert_eq!(parse_hex_color("00ff00"), Ok([0, 0xff, 0]));
        assert!(parse_hex_color("#fff").is_err());
        assert!(parse_hex_color("zzzzzz").is_err());
    }

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
