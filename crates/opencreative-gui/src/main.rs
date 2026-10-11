//! OpenCreative desktop app: a dashboard of live readings and controls for
//! what the cooler's screen shows, with a live preview.
//!
//! The app is a client of the daemon, which owns the USB device. If no daemon
//! is running, it starts one in-process for as long as the window is open.

// No console window alongside the app on Windows (debug builds keep it for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod pages;
mod prefs;
mod sensors;
#[cfg(target_os = "linux")]
mod service;
mod theme;
mod widgets;

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Frame, Margin, Sense, Stroke, pos2, vec2};
use opencreative_config::{Config, Rgb, SceneConfig};
use opencreative_core::Player;
use opencreative_daemon::Daemon;
use opencreative_ipc::{self as ipc, Request, Response};
use opencreative_protocol::pixel;
use opencreative_render::Rotation;

use crate::icons::Icon;
use crate::prefs::Appearance;
use crate::sensors::SensorHistory;
use crate::theme::Palette;

/// How often to ask the daemon for its state.
const STATUS_POLL: Duration = Duration::from_secs(2);
/// Poll faster while waiting for a daemon to come up.
const CONNECT_POLL: Duration = Duration::from_millis(250);
/// How long to wait for the system service before running one in-window.
const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(10);
const DEVICE_SCAN: Duration = Duration::from_secs(3);
const TOAST_FOR: Duration = Duration::from_secs(4);
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const SIDEBAR_WIDTH: f32 = 212.0;
const TITLE_HEIGHT: f32 = 40.0;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,opencreative=info"),
    )
    .init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenCreative")
            .with_app_id("opencreative")
            .with_icon(window_icon())
            .with_inner_size(window_size())
            .with_min_inner_size([1000.0, 680.0])
            // The app draws its own title bar.
            .with_decorations(false),
        ..Default::default()
    };
    eframe::run_native(
        "OpenCreative",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// Initial window size; `OPENCREATIVE_SIZE=WxH` overrides it (for screenshots).
fn window_size() -> [f32; 2] {
    std::env::var("OPENCREATIVE_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or([1180.0, 820.0])
}

fn window_icon() -> egui::IconData {
    const SIZE: u32 = 128;
    egui::IconData {
        rgba: icon_rgba(SIZE),
        width: SIZE,
        height: SIZE,
    }
}

/// The app icon as straight (not premultiplied) RGBA.
fn icon_rgba(size: u32) -> Vec<u8> {
    opencreative_render::icon::icon(size)
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

/// Desktop integration that only exists on Linux so far.
#[cfg(not(target_os = "linux"))]
mod service {
    pub type Result<T> = std::result::Result<T, String>;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ServiceState {
        pub installed: bool,
        pub enabled: bool,
        pub active: bool,
    }

    pub fn query() -> ServiceState {
        ServiceState {
            installed: false,
            enabled: false,
            active: false,
        }
    }
    pub fn enable() -> Result<()> {
        Err("not supported on this platform yet".into())
    }
    pub fn disable() -> Result<()> {
        enable()
    }
    pub fn udev_rule_installed() -> bool {
        true
    }
    pub fn udev_install_command() -> String {
        String::new()
    }
    pub fn install_udev_rule() -> Result<()> {
        enable()
    }
    pub fn desktop_entry_installed() -> bool {
        false
    }
    pub fn install_desktop_entry() -> Result<()> {
        enable()
    }
    pub fn remove_desktop_entry() -> Result<()> {
        enable()
    }
    pub fn power_rule_installed() -> bool {
        false
    }
    pub fn set_power_rule(_on: bool) -> Result<()> {
        enable()
    }
}

pub const SERVICE_SUPPORTED: bool = cfg!(target_os = "linux");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Devices,
    Device,
    Settings,
}

impl Page {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "dashboard" => Page::Dashboard,
            "devices" => Page::Devices,
            "device" => Page::Device,
            "settings" => Page::Settings,
            _ => return None,
        })
    }
}

/// How the app reaches the daemon.
pub enum Link {
    Connecting,
    /// A separate daemon (e.g. the systemd service).
    External,
    /// One we started inside this process.
    Embedded,
    /// The system service was just started; don't run one in-window yet.
    WaitingForService(Instant),
    Unavailable(String),
}

/// A supported cooler found on USB.
pub struct DeviceRow {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub serial: Option<String>,
}

/// The LCD preview, rendered locally exactly as the device would show it.
pub struct Preview {
    player: Option<Player>,
    /// What `player` was built from; rebuilt when any of it changes.
    key: Option<(SceneConfig, u16, bool)>,
    pub texture: Option<egui::TextureHandle>,
    pub size: (u32, u32),
    next_frame: Instant,
    pub error: Option<String>,
}

struct Toast {
    text: String,
    error: bool,
    since: Instant,
}

/// Crisp when enlarged (each panel pixel stays a sharp square), smooth when
/// shrunk into thumbnails.
pub const PIXEL_TEXTURE: egui::TextureOptions = egui::TextureOptions {
    magnification: egui::TextureFilter::Nearest,
    minification: egui::TextureFilter::Linear,
    wrap_mode: egui::TextureWrapMode::ClampToEdge,
    mipmap_mode: None,
};

/// Converts a rendered frame to the panel's RGB565 colors and back, so the
/// app shows exactly what the LCD can display.
pub fn panel_colors(rgba: &[u8], width: u32) -> Vec<u8> {
    let wire = pixel::rgba_to_rgb565_le(rgba, width as usize);
    pixel::rgb565_le_to_rgba(&wire)
}

/// Saves a screenshot of one page and exits (`OPENCREATIVE_SCREENSHOT`).
struct ScreenshotJob {
    path: PathBuf,
    requested: bool,
}

/// A slow job (a password prompt) running off the UI thread.
type Task = Receiver<(&'static str, service::Result<()>)>;

pub struct App {
    pub page: Page,
    client: Option<ipc::Client>,
    pub link: Link,
    embedded: Option<Daemon>,
    next_poll: Instant,
    /// Whether controls have been filled from the daemon's config yet.
    synced: bool,
    pub device: Option<String>,
    pub screen_on: bool,
    pub zen: bool,
    pub applied: Config,
    pub brightness: u8,
    /// The last choice in each mode, so switching modes restores it.
    pub digital_scene: SceneConfig,
    pub media_scene: SceneConfig,
    pub preview: Preview,
    pub skin_thumbs: pages::device::SkinThumbs,
    pub history: SensorHistory,
    pub gpu_name: Option<String>,
    pub devices: Vec<DeviceRow>,
    devices_next: Instant,
    pub service: Option<service::ServiceState>,
    pub picking_file: Option<Receiver<Option<PathBuf>>>,
    pub task: Option<Task>,
    toast: Option<Toast>,
    pub prefs: prefs::Prefs,
    logo: Option<egui::TextureHandle>,
    screenshot: Option<ScreenshotJob>,
    started: Instant,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut prefs = prefs::load();
        // For screenshots: show the integration without saving the choice.
        if std::env::var_os("OPENCREATIVE_CLAUDE").is_some() {
            prefs.claude_usage = true;
        }
        let mut appearance = prefs.appearance;
        if let Ok(theme) = std::env::var("OPENCREATIVE_THEME") {
            appearance = match theme.as_str() {
                "dark" => Appearance::Dark,
                "light" => Appearance::Light,
                _ => appearance,
            };
        }
        cc.egui_ctx.set_theme(appearance.preference());

        let page = std::env::var("OPENCREATIVE_PAGE")
            .ok()
            .and_then(|p| Page::parse(&p))
            .unwrap_or(Page::Dashboard);
        let screenshot = std::env::var_os("OPENCREATIVE_SCREENSHOT").map(|path| ScreenshotJob {
            path: path.into(),
            requested: false,
        });

        let applied = Config::default();
        Self {
            page,
            client: None,
            link: Link::Connecting,
            embedded: None,
            next_poll: Instant::now(),
            synced: false,
            device: None,
            screen_on: true,
            zen: false,
            brightness: 100,
            digital_scene: SceneConfig::default(),
            media_scene: default_media_scene(),
            applied,
            preview: Preview {
                player: None,
                key: None,
                texture: None,
                size: (0, 0),
                next_frame: Instant::now(),
                error: None,
            },
            skin_thumbs: Default::default(),
            history: SensorHistory::new(),
            gpu_name: gl_renderer(cc),
            devices: Vec::new(),
            devices_next: Instant::now(),
            service: None,
            picking_file: None,
            task: None,
            toast: None,
            prefs: prefs::Prefs {
                appearance,
                ..prefs
            },
            logo: None,
            screenshot,
            started: Instant::now(),
        }
    }

    /// On first launch, start with the computer: enable the background
    /// service (where supported) so the cooler screen runs from login.
    fn first_run(&mut self) {
        if self.prefs.first_run_done || self.screenshot.is_some() {
            return;
        }
        self.prefs.first_run_done = true;
        if SERVICE_SUPPORTED && !service::query().enabled {
            self.enable_service();
        }
        self.save_prefs();
    }

    pub fn save_prefs(&mut self) {
        if let Err(e) = prefs::save(&self.prefs) {
            self.fail(format!("saving preferences: {e}"));
        }
    }

    /// Connects to (or starts) the daemon and refreshes its state.
    fn poll(&mut self) {
        if Instant::now() < self.next_poll {
            return;
        }
        if self.page == Page::Settings {
            self.service = Some(service::query());
        }
        if self.client.is_none() {
            self.connect();
        }
        let fast = self.client.is_none() && !matches!(self.link, Link::Unavailable(_));
        self.next_poll = Instant::now() + if fast { CONNECT_POLL } else { STATUS_POLL };
        let Some(client) = self.client.as_mut() else {
            return;
        };
        match client.request(&Request::Status) {
            Ok(Response::Status(status)) => {
                if !self.synced || Some(self.brightness) == self.applied.brightness {
                    self.brightness = status.config.brightness.unwrap_or(100);
                }
                if is_digital(&status.config.scene) {
                    self.digital_scene = status.config.scene.clone();
                } else {
                    self.media_scene = status.config.scene.clone();
                }
                self.synced = true;
                self.device = status.device;
                self.screen_on = status.screen_on;
                self.zen = status.zen;
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
        match ipc::Client::connect(&ipc::default_socket_path()) {
            Ok(Some(client)) => {
                self.client = Some(client);
                self.link = if self.embedded.is_some() {
                    Link::Embedded
                } else {
                    Link::External
                };
            }
            Ok(None) => {
                if let Link::WaitingForService(deadline) = self.link
                    && Instant::now() < deadline
                {
                    return;
                }
                if self.embedded.is_none() {
                    self.start_embedded();
                }
            }
            Err(e) => self.link = Link::Unavailable(e.to_string()),
        }
    }

    /// No daemon is running: run one here so the device still has an owner.
    fn start_embedded(&mut self) {
        match opencreative_daemon::spawn(None) {
            Ok(daemon) => {
                self.embedded = Some(daemon);
                self.link = Link::Embedded;
                self.next_poll = Instant::now();
            }
            Err(e) => self.link = Link::Unavailable(format!("{e:#}")),
        }
    }

    /// Releases the device and socket held by the in-window daemon.
    fn stop_embedded(&mut self) {
        self.client = None;
        if let Some(daemon) = self.embedded.take()
            && let Err(e) = daemon.stop()
        {
            self.fail(format!("stopping the in-window service: {e:#}"));
        }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Sends a request; returns whether the daemon accepted it.
    pub fn send(&mut self, request: Request) -> bool {
        let Some(client) = self.client.as_mut() else {
            self.fail("not connected to the OpenCreative service".into());
            return false;
        };
        match client.request(&request) {
            Ok(Response::Ok) => {
                self.next_poll = Instant::now() + Duration::from_millis(300);
                true
            }
            Ok(Response::Error { message }) => {
                self.fail(message);
                false
            }
            Ok(other) => {
                self.fail(format!("unexpected daemon response: {other:?}"));
                false
            }
            Err(e) => {
                self.client = None;
                self.fail(format!("lost the daemon: {e}"));
                false
            }
        }
    }

    /// Shows `scene` on the cooler now and remembers it for its mode.
    pub fn set_scene(&mut self, mut scene: SceneConfig) {
        // The daemon has its own working directory.
        if let SceneConfig::Image { path, .. } = &mut scene {
            match fs::canonicalize(&*path) {
                Ok(absolute) => *path = absolute,
                Err(e) => return self.fail(format!("{}: {e}", path.display())),
            }
        }
        if is_digital(&scene) {
            self.digital_scene = scene.clone();
        } else {
            self.media_scene = scene.clone();
        }
        if scene != self.applied.scene
            && self.send(Request::SetScene {
                scene: scene.clone(),
            })
        {
            // Show it right away; the next status poll confirms it.
            self.applied.scene = scene;
        }
    }

    pub fn set_rotation(&mut self, degrees: u16) {
        if degrees != self.applied.rotation && self.send(Request::SetRotation { degrees }) {
            self.applied.rotation = degrees;
        }
    }

    pub fn set_zen(&mut self, on: bool) {
        if self.send(Request::SetZen { on }) {
            self.zen = on;
        }
    }

    #[allow(dead_code)] // The app offers Zen mode; the CLI keeps `screen off`.
    pub fn set_screen(&mut self, on: bool) {
        if self.send(Request::SetScreen { on }) {
            self.screen_on = on;
        }
    }

    pub fn set_brightness(&mut self, percent: u8) {
        if self.send(Request::SetBrightness { percent }) {
            self.applied.brightness = Some(percent);
        }
    }

    pub fn enable_service(&mut self) {
        // The service's daemon needs the device and socket this window holds.
        self.stop_embedded();
        match service::enable() {
            Ok(()) => {
                self.link = Link::WaitingForService(Instant::now() + SERVICE_START_TIMEOUT);
                self.notify("Running in the background.");
            }
            Err(e) => {
                self.link = Link::Connecting;
                self.fail(format!("starting the background service: {e}"));
            }
        }
        self.service = Some(service::query());
        self.next_poll = Instant::now();
    }

    pub fn disable_service(&mut self) {
        match service::disable() {
            Ok(()) => {
                self.client = None;
                self.link = Link::Connecting;
                self.notify(
                    "Background service stopped; this window drives the screen while open.",
                );
            }
            Err(e) => self.fail(format!("stopping the background service: {e}")),
        }
        self.service = Some(service::query());
        // Give the service a moment to release the device.
        self.next_poll = Instant::now() + Duration::from_millis(500);
    }

    pub fn run_task(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        job: fn() -> service::Result<()>,
    ) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send((label, job()));
            ctx.request_repaint();
        });
        self.task = Some(rx);
    }

    pub fn browse_for_image(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let dialog = rfd::AsyncFileDialog::new()
                .set_title("Choose an image or GIF")
                .add_filter("Images", &IMAGE_EXTENSIONS);
            let picked = pollster::block_on(dialog.pick_file()).map(|f| f.path().to_owned());
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        self.picking_file = Some(rx);
    }

    pub fn show_image(&mut self, path: PathBuf) {
        let fit = match &self.media_scene {
            SceneConfig::Image { fit, .. } => *fit,
            _ => Default::default(),
        };
        self.set_scene(SceneConfig::Image { path, fit });
        self.page = Page::Device;
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            error: false,
            since: Instant::now(),
        });
    }

    pub fn fail(&mut self, text: String) {
        log::warn!("{text}");
        self.toast = Some(Toast {
            text,
            error: true,
            since: Instant::now(),
        });
    }

    /// Collects results from the file dialog and background tasks.
    fn poll_background(&mut self) {
        if let Some(rx) = &self.picking_file {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    self.picking_file = None;
                    self.show_image(path);
                }
                Ok(None) | Err(TryRecvError::Disconnected) => self.picking_file = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.task {
            match rx.try_recv() {
                Ok((label, Ok(()))) => {
                    self.task = None;
                    self.notify(format!("{label}: done."));
                }
                Ok((label, Err(e))) => {
                    self.task = None;
                    self.fail(format!("{label}: {e}"));
                }
                Err(TryRecvError::Disconnected) => self.task = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            self.show_image(file.path().to_owned());
        }
    }

    fn scan_devices(&mut self) {
        if Instant::now() < self.devices_next {
            return;
        }
        self.devices_next = Instant::now() + DEVICE_SCAN;
        // Listing only enumerates USB; it never claims the device.
        if let Ok(found) = opencreative_device::discover() {
            self.devices = found
                .iter()
                .map(|f| DeviceRow {
                    name: f.spec.name,
                    width: f.spec.width,
                    height: f.spec.height,
                    serial: f.serial().map(str::to_owned),
                })
                .collect();
        }
    }

    /// Re-renders the preview when the scene changes or a frame is due.
    fn update_preview(&mut self, ctx: &egui::Context) {
        let p = &mut self.preview;
        let key = (self.applied.scene.clone(), self.applied.rotation, self.zen);
        let rotation = Rotation::from_degrees(self.applied.rotation).unwrap_or_default();
        let spec = &opencreative_device::SUPPORTED[0];
        // Upright, as it looks on the mounted pump.
        p.size = rotation.logical_size(spec.width, spec.height);

        if p.key.as_ref() != Some(&key) {
            let scene = if key.2 {
                Ok(opencreative_core::Scene::Zen)
            } else {
                key.0.to_scene()
            };
            p.player = match scene {
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
                    // Exactly what the panel shows: its 16-bit color.
                    let rgba = panel_colors(out.frame.data(), p.size.0);
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [p.size.0 as usize, p.size.1 as usize],
                        &rgba,
                    );
                    match &mut p.texture {
                        Some(texture) => texture.set(image, PIXEL_TEXTURE),
                        None => {
                            p.texture = Some(ctx.load_texture("preview", image, PIXEL_TEXTURE));
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
        ctx.request_repaint_after(p.next_frame.saturating_duration_since(now));
    }

    fn logo(&mut self, ctx: &egui::Context) -> egui::TextureId {
        self.logo
            .get_or_insert_with(|| {
                let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &icon_rgba(64));
                ctx.load_texture("logo", image, egui::TextureOptions::LINEAR)
            })
            .id()
    }

    /// A device's display name. The LM240 and LM360 share a USB ID, so the
    /// model comes from the user's choice on the Devices page.
    pub fn device_title<'a>(&self, name: &'a str) -> &'a str {
        if !name.contains("LM Series") {
            return name;
        }
        match self.prefs.model {
            prefs::Model::Lm240 => "DeepCool LM240",
            prefs::Model::Lm360 => "DeepCool LM360",
        }
    }

    /// What the app is doing, as a dot color and a sentence for its tooltip.
    fn status(&self, p: &Palette) -> (Color32, &'static str) {
        match (&self.link, &self.device) {
            (Link::Unavailable(_), _) => (p.bad, "The background service is unavailable"),
            (Link::Connecting | Link::WaitingForService(_), _) => (p.faint, "Connecting…"),
            (_, None) => (p.faint, "No cooler connected"),
            (_, Some(_)) if !self.screen_on => (p.faint, "The cooler's screen is off"),
            (_, Some(_)) if self.zen => (p.accent, "Zen mode is on"),
            (Link::External, Some(_)) => (
                p.good,
                "Running in the background: the screen keeps updating when this window is closed",
            ),
            (Link::Embedded, Some(_)) => (
                p.warn,
                "Running in this window: the screen stops updating when it closes",
            ),
        }
    }

    /// The custom title bar: drag to move, double-click to maximize, the
    /// status dot and window buttons on the right.
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::of(ui.ctx());
        let rect = ui.max_rect();
        let drag = ui.interact(rect, egui::Id::new("title-drag"), Sense::click_and_drag());
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        let logo = self.logo(ui.ctx());
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            ui.add(egui::Image::new((logo, vec2(20.0, 20.0))));
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("OpenCreative")
                    .font(theme::semibold(13.0))
                    .color(p.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if window_button(ui, Icon::Close, true).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let (icon, tip) = if maximized {
                    (Icon::Restore, "Restore")
                } else {
                    (Icon::Maximize, "Maximize")
                };
                if window_button(ui, icon, false).on_hover_text(tip).clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                if window_button(ui, Icon::Minimize, false)
                    .on_hover_text("Minimize")
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                ui.add_space(10.0);
                let (color, text) = self.status(&p);
                let (dot, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                if response.hovered() {
                    ui.painter().circle_filled(dot.center(), 10.0, p.raised);
                }
                ui.painter()
                    .circle_filled(dot.center(), 6.0, color.gamma_multiply(0.25));
                ui.painter().circle_filled(dot.center(), 3.5, color);
                if response
                    .on_hover_text(format!("{text}. Click for service settings."))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.page = Page::Settings;
                }
            });
        });
    }

    /// Navigation with labels, plus the connected cooler.
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::of(ui.ctx());
        ui.add_space(14.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        for (page, icon, label) in [
            (Page::Dashboard, Icon::Dashboard, "Dashboard"),
            (Page::Devices, Icon::Devices, "Devices"),
            (Page::Device, Icon::Cooler, "Cooler screen"),
        ] {
            self.nav_item(ui, &p, page, icon, label);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(12.0);
            self.nav_item(ui, &p, Page::Settings, Icon::Settings, "Settings");
            ui.add_space(8.0);
            if let Some(device) = self.device.clone() {
                let device = self.device_title(&device).to_owned();
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new(device).small().color(p.faint));
                });
            }
        });
    }

    fn nav_item(&mut self, ui: &mut egui::Ui, p: &Palette, page: Page, icon: Icon, label: &str) {
        let width = ui.available_width();
        let (outer, response) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
        let rect = outer.shrink2(vec2(10.0, 1.0));
        let selected = self.page == page;
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 8.0, p.raised);
            let bar = egui::Rect::from_center_size(
                pos2(rect.left() + 1.5, rect.center().y),
                vec2(3.0, 16.0),
            );
            painter.rect_filled(bar, 2.0, p.accent);
        } else if response.hovered() {
            painter.rect_filled(rect, 8.0, p.raised.gamma_multiply(0.6));
        }
        let color = if selected { p.text } else { p.muted };
        let icon_rect = egui::Rect::from_center_size(
            pos2(rect.left() + 22.0, rect.center().y),
            vec2(18.0, 18.0),
        );
        icons::paint(
            ui,
            icon_rect,
            icon,
            if selected { p.accent } else { p.muted },
        );
        ui.painter().text(
            pos2(rect.left() + 42.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            if selected {
                theme::medium(14.0)
            } else {
                theme::regular(14.0)
            },
            color,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.page = page;
        }
    }

    /// Transient messages in the bottom-right corner.
    fn toasts(&mut self, ctx: &egui::Context) {
        let p = Palette::of(ctx);
        let Some((text, error)) = self
            .toast
            .as_ref()
            .filter(|t| t.since.elapsed() < TOAST_FOR)
            .map(|t| (t.text.clone(), t.error))
        else {
            return;
        };
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-20.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(p.raised)
                    .stroke(Stroke::new(1.0, if error { p.bad } else { p.border }))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(
                                dot.center(),
                                3.5,
                                if error { p.bad } else { p.accent },
                            );
                            ui.label(egui::RichText::new(text).color(p.text));
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    /// Without the system frame, the window's edges resize it.
    fn resize_edges(&self, ctx: &egui::Context) {
        const EDGE: f32 = 6.0;
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let screen = ctx.content_rect();
        let Some(pos) = ctx.pointer_hover_pos() else {
            return;
        };
        let (l, r) = (pos.x - screen.left() < EDGE, screen.right() - pos.x < EDGE);
        let (t, b) = (pos.y - screen.top() < EDGE, screen.bottom() - pos.y < EDGE);
        use egui::ResizeDirection as D;
        let (dir, cursor) = match (l, r, t, b) {
            (true, _, true, _) => (D::NorthWest, egui::CursorIcon::ResizeNorthWest),
            (_, true, true, _) => (D::NorthEast, egui::CursorIcon::ResizeNorthEast),
            (true, _, _, true) => (D::SouthWest, egui::CursorIcon::ResizeSouthWest),
            (_, true, _, true) => (D::SouthEast, egui::CursorIcon::ResizeSouthEast),
            (true, ..) => (D::West, egui::CursorIcon::ResizeWest),
            (_, true, ..) => (D::East, egui::CursorIcon::ResizeEast),
            (_, _, true, _) => (D::North, egui::CursorIcon::ResizeNorth),
            (_, _, _, true) => (D::South, egui::CursorIcon::ResizeSouth),
            _ => return,
        };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(job) = self.screenshot.as_mut() else {
            return;
        };
        // Give sensors, the daemon and the preview time to fill in.
        if !job.requested && self.started.elapsed() > Duration::from_secs(4) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            job.requested = true;
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            let saved = tiny_skia_pixmap(bytes, w as u32, h as u32)
                .map(|pm| pm.save_png(&job.path).is_ok())
                .unwrap_or(false);
            log::info!("screenshot saved: {saved} ({})", job.path.display());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn tiny_skia_pixmap(
    premultiplied_rgba: Vec<u8>,
    w: u32,
    h: u32,
) -> Option<opencreative_render::Pixmap> {
    let size = tiny_skia::IntSize::from_wh(w, h)?;
    opencreative_render::Pixmap::from_vec(premultiplied_rgba, size)
}

/// GPU name from the OpenGL driver, e.g. "Intel(R) Graphics (RPL-S)".
fn gl_renderer(cc: &eframe::CreationContext<'_>) -> Option<String> {
    use eframe::glow::HasContext;
    let gl = cc.gl.as_ref()?;
    // SAFETY: querying a string parameter on the current, valid GL context.
    let name = unsafe { gl.get_parameter_string(eframe::glow::RENDERER) };
    let name = name.trim_start_matches("Mesa ").trim();
    (!name.is_empty() && !name.contains("llvmpipe")).then(|| name.to_owned())
}

pub fn is_digital(scene: &SceneConfig) -> bool {
    matches!(
        scene,
        SceneConfig::Panels { .. } | SceneConfig::Monitor { .. }
    )
}

fn default_media_scene() -> SceneConfig {
    SceneConfig::Solid {
        color: Rgb([0x0d, 0x9b, 0x94]),
    }
}

/// A title-bar button: an icon on a square that lights up on hover (red
/// for close).
fn window_button(ui: &mut egui::Ui, icon: Icon, close: bool) -> egui::Response {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(44.0, TITLE_HEIGHT), Sense::click());
    let hovered = response.hovered();
    if hovered {
        let fill = if close { p.bad } else { p.raised };
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    let color = if hovered && close {
        Color32::WHITE
    } else {
        p.muted
    };
    icons::paint(
        ui,
        egui::Rect::from_center_size(rect.center(), vec2(15.0, 15.0)),
        icon,
        color,
    );
    response
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.first_run();
        self.poll();
        self.poll_background();
        self.handle_dropped_files(&ctx);
        if matches!(self.page, Page::Device | Page::Devices) {
            self.update_preview(&ctx);
        }
        if self.page == Page::Devices {
            self.scan_devices();
            ctx.request_repaint_after(DEVICE_SCAN);
        }
        opencreative_sensors::set_claude_enabled(self.prefs.claude_usage);
        let until_sample = self.history.tick();
        let until_poll = self.next_poll.saturating_duration_since(Instant::now());
        ctx.request_repaint_after(until_sample.min(until_poll));
        self.handle_screenshot(&ctx);

        let p = Palette::of(&ctx);
        self.resize_edges(&ctx);
        egui::Panel::top("title-bar")
            .resizable(false)
            .exact_size(TITLE_HEIGHT)
            .frame(Frame::new().fill(p.sidebar))
            .show(ui, |ui| self.title_bar(ui));
        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(SIDEBAR_WIDTH)
            .frame(
                Frame::new()
                    .fill(p.sidebar)
                    .stroke(Stroke::new(1.0, p.border)),
            )
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin::same(28)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Dashboard => pages::dashboard::show(self, ui),
                        Page::Devices => pages::devices::show(self, ui),
                        Page::Device => pages::device::show(self, ui),
                        Page::Settings => pages::settings::show(self, ui),
                    });
            });
        self.toasts(&ctx);
        // Without the system frame: a hairline under the title bar and
        // around the window.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window-frame"),
        ));
        let screen = ctx.content_rect();
        painter.hline(
            screen.x_range(),
            screen.top() + TITLE_HEIGHT - 0.5,
            Stroke::new(1.0, p.border),
        );
        if !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            painter.rect_stroke(
                screen.shrink(0.5),
                0.0,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Release the device and remove the socket instead of leaving them
        // to process exit.
        self.stop_embedded();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_icon_has_the_right_size() {
        let icon = window_icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

    #[test]
    fn digital_and_media_scenes_are_told_apart() {
        assert!(is_digital(&SceneConfig::default()));
        assert!(!is_digital(&default_media_scene()));
    }
}
