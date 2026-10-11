//! OpenCreative desktop app: choose what the cooler shows, with a live preview.
//!
//! The app is a client of the daemon, which owns the USB device. If no daemon
//! is running, it starts one in-process for as long as the window is open.

// No console window alongside the app on Windows (debug builds keep it for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use opencreative_config::{Config, FitConfig, Rgb, SceneConfig};
use opencreative_core::Player;
use opencreative_ipc::{self as ipc, Request, Response};
use opencreative_protocol::pixel;
use opencreative_render::Rotation;

/// How often to ask the daemon for its state.
const STATUS_POLL: Duration = Duration::from_secs(2);
/// Poll faster while waiting for an embedded daemon to come up.
const CONNECT_POLL: Duration = Duration::from_millis(250);
const PREVIEW_SCALE: f32 = 2.0;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,opencreative=info"),
    )
    .init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenCreative")
            .with_inner_size([1000.0, 640.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OpenCreative",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}

/// How the app reaches the daemon.
enum Link {
    Connecting,
    /// A separate daemon (e.g. the systemd service).
    External,
    /// One we started inside this process.
    Embedded,
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Monitor,
    Image,
    Solid,
}

/// The scene being edited, before it is applied.
#[derive(Debug, Clone, PartialEq)]
struct Draft {
    kind: Kind,
    interval_secs: f64,
    image: String,
    fit: FitConfig,
    color: [u8; 3],
}

impl Draft {
    fn from_scene(scene: &SceneConfig) -> Self {
        let mut draft = Self {
            kind: Kind::Monitor,
            interval_secs: 1.0,
            image: String::new(),
            fit: FitConfig::Cover,
            color: [0x22, 0xd3, 0xee],
        };
        match scene {
            SceneConfig::Monitor { interval_secs } => draft.interval_secs = *interval_secs,
            SceneConfig::Image { path, fit } => {
                draft.kind = Kind::Image;
                draft.image = path.display().to_string();
                draft.fit = *fit;
            }
            SceneConfig::Solid { color } => {
                draft.kind = Kind::Solid;
                draft.color = color.0;
            }
        }
        draft
    }

    fn to_scene(&self) -> SceneConfig {
        match self.kind {
            Kind::Monitor => SceneConfig::Monitor {
                interval_secs: self.interval_secs,
            },
            Kind::Image => SceneConfig::Image {
                path: PathBuf::from(self.image.trim()),
                fit: self.fit,
            },
            Kind::Solid => SceneConfig::Solid {
                color: Rgb(self.color),
            },
        }
    }
}

/// Renders the draft scene locally, exactly as the device would.
struct Preview {
    player: Option<Player>,
    /// What `player` was built from; rebuilt when any of it changes.
    key: Option<(SceneConfig, u16, bool)>,
    accurate: bool,
    texture: Option<egui::TextureHandle>,
    size: (u32, u32),
    next_frame: Instant,
    error: Option<String>,
}

struct App {
    client: Option<ipc::Client>,
    link: Link,
    embedded_error: Arc<Mutex<Option<String>>>,
    next_poll: Instant,
    /// Whether controls have been filled from the daemon's config yet.
    synced: bool,
    device: Option<String>,
    applied: Config,
    draft: Draft,
    brightness: u8,
    screen_on: bool,
    preview: Preview,
    message: Option<(String, bool)>,
}

impl App {
    fn new() -> Self {
        let applied = Config::default();
        Self {
            client: None,
            link: Link::Connecting,
            embedded_error: Arc::default(),
            next_poll: Instant::now(),
            synced: false,
            device: None,
            draft: Draft::from_scene(&applied.scene),
            brightness: 100,
            screen_on: true,
            applied,
            preview: Preview {
                player: None,
                key: None,
                accurate: true,
                texture: None,
                size: (0, 0),
                next_frame: Instant::now(),
                error: None,
            },
            message: None,
        }
    }

    /// Connects to (or starts) the daemon and refreshes its state.
    fn poll(&mut self) {
        if Instant::now() < self.next_poll {
            return;
        }
        if self.client.is_none() {
            self.connect();
        }
        self.next_poll = Instant::now()
            + if self.client.is_some() {
                STATUS_POLL
            } else {
                CONNECT_POLL
            };
        let Some(client) = self.client.as_mut() else {
            return;
        };
        match client.request(&Request::Status) {
            Ok(Response::Status(status)) => {
                // Follow outside changes (e.g. from the CLI) unless the user
                // has unapplied edits.
                if !self.synced || self.draft == Draft::from_scene(&self.applied.scene) {
                    self.draft = Draft::from_scene(&status.config.scene);
                }
                if !self.synced || Some(self.brightness) == self.applied.brightness {
                    self.brightness = status.config.brightness.unwrap_or(100);
                }
                self.synced = true;
                self.device = status.device;
                self.screen_on = status.screen_on;
                self.applied = status.config;
            }
            Ok(other) => self.fail(format!("unexpected daemon response: {other:?}")),
            Err(e) => {
                self.client = None;
                self.fail(format!("lost the daemon: {e}"));
            }
        }
    }

    fn connect(&mut self) {
        if let Some(e) = self.embedded_error.lock().expect("lock").take() {
            self.link = Link::Unavailable(e);
        }
        match ipc::Client::connect(&ipc::default_socket_path()) {
            Ok(Some(client)) => {
                self.client = Some(client);
                if !matches!(self.link, Link::Embedded) {
                    self.link = Link::External;
                }
            }
            Ok(None) if matches!(self.link, Link::Connecting) => {
                // No daemon: run one here so the device still has an owner.
                let errors = Arc::clone(&self.embedded_error);
                thread::spawn(move || {
                    if let Err(e) = opencreative_daemon::run(None) {
                        log::error!("embedded daemon: {e:#}");
                        *errors.lock().expect("lock") = Some(format!("{e:#}"));
                    }
                });
                self.link = Link::Embedded;
            }
            Ok(None) => {}
            Err(e) => self.link = Link::Unavailable(e.to_string()),
        }
    }

    fn send(&mut self, request: Request) {
        let Some(client) = self.client.as_mut() else {
            self.fail("not connected to the daemon".into());
            return;
        };
        match client.request(&request) {
            Ok(Response::Ok) => {
                self.message = Some(("Applied.".into(), false));
                self.next_poll = Instant::now();
            }
            Ok(Response::Error { message }) => self.fail(message),
            Ok(other) => self.fail(format!("unexpected daemon response: {other:?}")),
            Err(e) => {
                self.client = None;
                self.fail(format!("lost the daemon: {e}"));
            }
        }
    }

    fn fail(&mut self, message: String) {
        log::warn!("{message}");
        self.message = Some((message, true));
    }

    fn apply_scene(&mut self) {
        let mut scene = self.draft.to_scene();
        // The daemon has its own working directory.
        if let SceneConfig::Image { path, .. } = &mut scene {
            match fs::canonicalize(&*path) {
                Ok(absolute) => *path = absolute,
                Err(e) => return self.fail(format!("{}: {e}", path.display())),
            }
        }
        self.send(Request::SetScene { scene });
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            self.draft.kind = Kind::Image;
            self.draft.image = file.path().display().to_string();
        }
    }

    /// Re-renders the preview when the draft changes or a frame is due.
    fn update_preview(&mut self, ctx: &egui::Context) {
        let p = &mut self.preview;
        let key = (self.draft.to_scene(), self.applied.rotation, p.accurate);
        let rotation = Rotation::from_degrees(self.applied.rotation).unwrap_or_default();
        let spec = &opencreative_device::SUPPORTED[0];
        // Upright, as it looks on the mounted pump.
        p.size = rotation.logical_size(spec.width, spec.height);

        if p.key.as_ref() != Some(&key) {
            p.player = match key.0.to_scene() {
                Ok(scene) => {
                    p.error = None;
                    Some(Player::new(scene, Rotation::R0))
                }
                Err(e) => {
                    p.error = Some(e.to_string());
                    None
                }
            };
            p.key = Some(key);
            p.next_frame = Instant::now();
        }

        let now = Instant::now();
        if now >= p.next_frame
            && let Some(player) = p.player.as_mut()
        {
            match player.next(p.size.0, p.size.1) {
                Ok(out) => {
                    let mut rgba = out.frame.data().to_vec();
                    if p.accurate {
                        let wire = pixel::rgba_to_rgb565_le(&rgba, p.size.0 as usize);
                        rgba = pixel::rgb565_le_to_rgba(&wire);
                    }
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [p.size.0 as usize, p.size.1 as usize],
                        &rgba,
                    );
                    match &mut p.texture {
                        Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
                        None => {
                            p.texture = Some(ctx.load_texture(
                                "preview",
                                image,
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                    }
                    p.next_frame = now + out.hold.unwrap_or(Duration::from_secs(3600));
                }
                Err(e) => {
                    p.error = Some(e.to_string());
                    p.player = None;
                }
            }
        }
        let until_frame = p.next_frame.saturating_duration_since(now);
        let until_poll = self.next_poll.saturating_duration_since(now);
        ctx.request_repaint_after(until_frame.min(until_poll));
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("OpenCreative");
            ui.separator();
            match &self.device {
                Some(name) => ui.colored_label(egui::Color32::LIGHT_GREEN, format!("● {name}")),
                None => ui.colored_label(egui::Color32::GRAY, "○ no cooler connected"),
            };
            ui.separator();
            match &self.link {
                Link::Connecting => ui.label("connecting…"),
                Link::External => ui.label("background service running"),
                Link::Embedded => ui.label("service running in this window — the screen stops updating when it closes")
                    .on_hover_text("Enable the systemd service to keep the screen updated:\nsystemctl --user enable --now opencreative"),
                Link::Unavailable(e) => ui.colored_label(egui::Color32::LIGHT_RED, format!("service unavailable: {e}")),
            };
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Screen");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.draft.kind, Kind::Monitor, "System monitor");
            ui.radio_value(&mut self.draft.kind, Kind::Image, "Image / GIF");
            ui.radio_value(&mut self.draft.kind, Kind::Solid, "Color");
        });
        ui.add_space(6.0);
        match self.draft.kind {
            Kind::Monitor => {
                ui.add(
                    egui::Slider::new(&mut self.draft.interval_secs, 0.25..=5.0)
                        .text("seconds between updates"),
                );
            }
            Kind::Image => {
                ui.label("File (or drop one onto the window):");
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.image)
                        .hint_text("/path/to/image.gif"),
                );
                egui::ComboBox::from_label("Fit")
                    .selected_text(fit_label(self.draft.fit))
                    .show_ui(ui, |ui| {
                        for fit in [FitConfig::Cover, FitConfig::Contain, FitConfig::Stretch] {
                            ui.selectable_value(&mut self.draft.fit, fit, fit_label(fit));
                        }
                    });
            }
            Kind::Solid => {
                ui.horizontal(|ui| {
                    ui.label("Color");
                    ui.color_edit_button_srgb(&mut self.draft.color);
                });
            }
        }
        ui.add_space(6.0);
        let dirty = self.draft.to_scene() != self.applied.scene;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(dirty && self.client.is_some(), egui::Button::new("Apply"))
                .clicked()
            {
                self.apply_scene();
            }
            if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                self.draft = Draft::from_scene(&self.applied.scene);
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Display");
        let slider = ui.add(egui::Slider::new(&mut self.brightness, 0..=100).text("brightness"));
        // Send once the user lets go, not on every pixel of a drag.
        if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
            self.send(Request::SetBrightness {
                percent: self.brightness,
            });
        }
        let mut rotation = self.applied.rotation;
        egui::ComboBox::from_label("Rotation")
            .selected_text(format!("{rotation}°"))
            .show_ui(ui, |ui| {
                for degrees in [0, 90, 180, 270] {
                    ui.selectable_value(&mut rotation, degrees, format!("{degrees}°"));
                }
            });
        if rotation != self.applied.rotation {
            self.send(Request::SetRotation { degrees: rotation });
        }
        let label = if self.screen_on {
            "Turn screen off"
        } else {
            "Turn screen on"
        };
        if ui
            .add_enabled(self.client.is_some(), egui::Button::new(label))
            .clicked()
        {
            self.send(Request::SetScreen {
                on: !self.screen_on,
            });
        }
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Preview");
            ui.checkbox(&mut self.preview.accurate, "Device-accurate colors")
                .on_hover_text("Show the panel's 16-bit color (RGB565 with dithering)");
        });
        let dirty = self.draft.to_scene() != self.applied.scene;
        ui.label(if dirty {
            "Unapplied changes — press Apply to send them to the cooler."
        } else {
            "This is what the cooler shows."
        });
        ui.add_space(8.0);
        if let Some(texture) = &self.preview.texture {
            let (w, h) = self.preview.size;
            let size = egui::vec2(w as f32, h as f32) * PREVIEW_SCALE;
            ui.image((texture.id(), size));
        }
        if let Some(e) = &self.preview.error {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
    }
}

fn fit_label(fit: FitConfig) -> &'static str {
    match fit {
        FitConfig::Cover => "Fill (crop)",
        FitConfig::Contain => "Fit (letterbox)",
        FitConfig::Stretch => "Stretch",
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        self.handle_dropped_files(&ctx);
        self.update_preview(&ctx);

        egui::Panel::top("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::bottom("message").show(ui, |ui| match &self.message {
            Some((text, true)) => ui.colored_label(egui::Color32::LIGHT_RED, text),
            Some((text, false)) => ui.label(text),
            None => ui.label(""),
        });
        egui::Panel::left("controls")
            .resizable(false)
            .exact_size(340.0)
            .show(ui, |ui| self.controls(ui));
        egui::CentralPanel::default().show(ui, |ui| self.preview(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_round_trips_every_scene() {
        for scene in [
            SceneConfig::Monitor { interval_secs: 2.5 },
            SceneConfig::Image {
                path: "/tmp/a.gif".into(),
                fit: FitConfig::Contain,
            },
            SceneConfig::Solid {
                color: Rgb([1, 2, 3]),
            },
        ] {
            assert_eq!(Draft::from_scene(&scene).to_scene(), scene);
        }
    }
}
