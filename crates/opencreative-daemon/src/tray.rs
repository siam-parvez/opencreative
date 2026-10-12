//! The background service's top-bar (tray) icon: a StatusNotifierItem over
//! D-Bus, shown by KDE, by GNOME with the AppIndicator extension (Ubuntu's
//! default), and by most other desktops. Its menu opens the app and offers
//! quick switches; every change goes through the same handling as the
//! app's requests, so it's saved and the app sees it.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::thread;

use ksni::blocking::{Handle, TrayMethods as _};
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};
use opencreative_config::{SceneConfig, SceneKind, ScreenTheme, SkinConfig};
use opencreative_ipc::Request;

use crate::Shared;

/// Brighter and Dimmer move in these steps, down to this level.
const BRIGHTNESS_STEP: u8 = 10;
const BRIGHTNESS_MIN: u8 = 10;
/// The app's three modes, as the menu lists them.
const KINDS: [(SceneKind, &str); 3] = [
    (SceneKind::Digital, "Live data"),
    (SceneKind::Media, "Pictures & video"),
    (SceneKind::Music, "Music"),
];
/// Looks for live data, as the menu lists them.
const LOOKS: [(Look, &str); 3] = [
    (Look::Skin(SkinConfig::Lines), "Lines"),
    (Look::Skin(SkinConfig::Hud), "HUD"),
    (Look::Gauges, "Gauges"),
];

/// A live-data look: one of the Panels skins, or the gauges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Look {
    Skin(SkinConfig),
    Gauges,
}

impl Look {
    /// The look `scene` shows, if it's live data.
    pub fn of(scene: &SceneConfig) -> Option<Look> {
        match scene {
            SceneConfig::Panels { skin, .. } => Some(Look::Skin(*skin)),
            SceneConfig::Monitor { .. } => Some(Look::Gauges),
            _ => None,
        }
    }
}

/// What the menu shows, read from the daemon when something changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct View {
    pub device: Option<String>,
    pub screen_on: bool,
    pub zen: bool,
    pub brightness: u8,
    pub kind: SceneKind,
    pub look: Option<Look>,
    pub theme: ScreenTheme,
    pub rotation: u16,
}

pub(crate) struct CoolerTray {
    shared: Arc<Shared>,
    view: View,
}

impl CoolerTray {
    /// Applies a request as the app would. The menu shows the new value
    /// at once: the playback thread applies it a moment later, and reading
    /// it back now would still give the old one.
    fn apply(&mut self, request: Request) {
        match request {
            Request::SetScreen { on } => self.view.screen_on = on,
            Request::SetZen { on } => self.view.zen = on,
            Request::SetBrightness { percent } => self.view.brightness = percent,
            Request::SetScene { ref scene } => {
                self.view.kind = scene.kind();
                self.view.look = Look::of(scene);
            }
            Request::SetScreenTheme { theme } => self.view.theme = theme,
            Request::SetRotation { degrees } => self.view.rotation = degrees,
            _ => {}
        }
        let _ = self.shared.handle(request);
    }

    /// Switches to the last scene of `kind`, as it was left.
    fn show_kind(&mut self, kind: SceneKind) {
        if self.view.kind == kind {
            return;
        }
        let scene = self
            .shared
            .config
            .lock()
            .expect("config lock")
            .recent
            .get(kind);
        self.apply(Request::SetScene { scene });
    }

    /// Switches live data to `look`, keeping the chosen readings and how
    /// often they refresh.
    fn show_look(&mut self, look: Look) {
        let (current, last) = {
            let config = self.shared.config.lock().expect("config lock");
            (config.scene.clone(), config.recent.get(SceneKind::Digital))
        };
        let interval_secs = [&current, &last]
            .into_iter()
            .find_map(|scene| match scene {
                SceneConfig::Panels { interval_secs, .. }
                | SceneConfig::Monitor { interval_secs } => Some(*interval_secs),
                _ => None,
            })
            .unwrap_or(1.0);
        let readings = [&current, &last, &SceneConfig::default()]
            .into_iter()
            .find_map(|scene| match scene {
                SceneConfig::Panels {
                    primary, secondary, ..
                } => Some((*primary, *secondary)),
                _ => None,
            });
        let scene = match (look, readings) {
            (Look::Gauges, _) | (Look::Skin(_), None) => SceneConfig::Monitor { interval_secs },
            (Look::Skin(skin), Some((primary, secondary))) => SceneConfig::Panels {
                skin,
                primary,
                secondary,
                interval_secs,
            },
        };
        self.apply(Request::SetScene { scene });
    }
}

/// Starts the icon. It appears once the desktop's tray host is up, even if
/// that's after the service started at login.
pub(crate) fn show(shared: Arc<Shared>) -> Option<Handle<CoolerTray>> {
    let view = shared.view();
    let tray = CoolerTray { shared, view };
    match tray.assume_sni_available(true).spawn() {
        Ok(handle) => Some(handle),
        Err(e) => {
            log::warn!("no top-bar icon: {e}");
            None
        }
    }
}

/// Brings the menu up to date (after a change made elsewhere).
pub(crate) fn refresh(handle: &Handle<CoolerTray>, view: View) {
    handle.update(|tray| {
        if tray.view != view {
            tray.view = view;
        }
    });
}

impl ksni::Tray for CoolerTray {
    fn id(&self) -> String {
        "opencreative".into()
    }

    fn title(&self) -> String {
        "OpenCreative".into()
    }

    fn icon_name(&self) -> String {
        // The installed one-color icon, which the desktop recolors to match
        // its panel; the bitmaps below where it isn't installed.
        "opencreative-symbolic".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        icons().clone()
    }

    fn tool_tip(&self) -> ToolTip {
        let description = match (&self.view.device, self.view.screen_on) {
            (None, _) => "No cooler connected".to_owned(),
            (Some(device), false) => format!("{device}: screen off"),
            (Some(device), true) if self.view.zen => format!("{device}: Zen mode"),
            (Some(device), true) => device.clone(),
        };
        ToolTip {
            title: "OpenCreative".into(),
            description,
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        open_app();
    }

    /// One flat list: the desktop closes the menu after every click and
    /// redraws submenus as they open, so each change is a single click on
    /// an item that never moves.
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let view = &self.view;
        let connected = view.device.is_some();
        let live = view.kind == SceneKind::Digital;
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: "Open OpenCreative".into(),
                activate: Box::new(|_: &mut Self| open_app()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            RadioGroup {
                selected: KINDS.iter().position(|(k, _)| *k == view.kind).unwrap_or(0),
                select: Box::new(|tray: &mut Self, i| {
                    tray.show_kind(KINDS[i.min(KINDS.len() - 1)].0);
                }),
                options: KINDS
                    .iter()
                    .map(|(_, label)| RadioItem {
                        label: (*label).into(),
                        enabled: connected,
                        ..Default::default()
                    })
                    .collect(),
            }
            .into(),
            MenuItem::Separator,
            // The live-data looks stay in place, greyed out in other modes.
            RadioGroup {
                selected: LOOKS
                    .iter()
                    .position(|(l, _)| Some(*l) == view.look)
                    .unwrap_or(0),
                select: Box::new(|tray: &mut Self, i| {
                    tray.show_look(LOOKS[i.min(LOOKS.len() - 1)].0);
                }),
                options: LOOKS
                    .iter()
                    .map(|(_, label)| RadioItem {
                        label: (*label).into(),
                        enabled: connected && live,
                        ..Default::default()
                    })
                    .collect(),
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Screen on".into(),
                checked: view.screen_on,
                enabled: connected,
                activate: Box::new(|tray: &mut Self| {
                    let on = !tray.view.screen_on;
                    tray.apply(Request::SetScreen { on });
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Zen mode".into(),
                checked: view.zen,
                enabled: connected,
                activate: Box::new(|tray: &mut Self| {
                    let on = !tray.view.zen;
                    tray.apply(Request::SetZen { on });
                }),
                ..Default::default()
            }
            .into(),
        ];
        items.extend([
            StandardItem {
                label: format!("Brighter ({}%)", view.brightness),
                enabled: connected && view.brightness < 100,
                activate: Box::new(|tray: &mut Self| {
                    let percent = step_brightness(tray.view.brightness, true);
                    tray.apply(Request::SetBrightness { percent });
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Dimmer".into(),
                enabled: connected && view.brightness > BRIGHTNESS_MIN,
                activate: Box::new(|tray: &mut Self| {
                    let percent = step_brightness(tray.view.brightness, false);
                    tray.apply(Request::SetBrightness { percent });
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Light screen colors".into(),
                checked: view.theme == ScreenTheme::Light,
                // Pictures and videos keep their own colors.
                enabled: connected && view.kind != SceneKind::Media,
                activate: Box::new(|tray: &mut Self| {
                    let theme = match tray.view.theme {
                        ScreenTheme::Dark => ScreenTheme::Light,
                        ScreenTheme::Light => ScreenTheme::Dark,
                    };
                    tray.apply(Request::SetScreenTheme { theme });
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: format!("Rotate 90° (now {}°)", view.rotation),
                enabled: connected,
                activate: Box::new(|tray: &mut Self| {
                    let degrees = (tray.view.rotation + 90) % 360;
                    tray.apply(Request::SetRotation { degrees });
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Hide this icon".into(),
                activate: Box::new(|tray: &mut Self| {
                    // Turned back on in the app's Settings.
                    tray.apply(Request::SetTray { on: false });
                }),
                ..Default::default()
            }
            .into(),
        ]);
        items
    }
}

/// The next brightness step up or down, in tens, never fully dark.
fn step_brightness(now: u8, up: bool) -> u8 {
    let step = if up {
        (now / BRIGHTNESS_STEP + 1) * BRIGHTNESS_STEP
    } else {
        now.saturating_sub(1) / BRIGHTNESS_STEP * BRIGHTNESS_STEP
    };
    step.clamp(BRIGHTNESS_MIN, 100)
}

/// The one-color icon at tray sizes, white (top bars are dark), as ARGB32
/// in network byte order.
fn icons() -> &'static Vec<Icon> {
    static ICONS: OnceLock<Vec<Icon>> = OnceLock::new();
    ICONS.get_or_init(|| {
        [16, 22, 24, 32, 48, 64]
            .into_iter()
            .map(|size| {
                let pm = crate::tray_icon::render(size, [255, 255, 255]);
                // Premultiplied RGBA to straight ARGB.
                let data = pm
                    .pixels()
                    .iter()
                    .flat_map(|p| {
                        let c = p.demultiply();
                        [c.alpha(), c.red(), c.green(), c.blue()]
                    })
                    .collect();
                Icon {
                    width: size as i32,
                    height: size as i32,
                    data,
                }
            })
            .collect()
    })
}

/// Opens the app window: the AppImage when running from one, else the
/// app installed next to this program, else whatever is on `PATH`.
fn open_app() {
    let program = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| {
            let beside = std::env::current_exe()
                .ok()?
                .with_file_name("opencreative-gui");
            beside.is_file().then_some(beside)
        })
        .unwrap_or_else(|| PathBuf::from("opencreative-gui"));
    match Command::new(&program).spawn() {
        // Reap it when it closes so it doesn't linger as a zombie.
        Ok(mut child) => {
            let _ = thread::Builder::new()
                .name("app-window".into())
                .spawn(move || child.wait());
        }
        Err(e) => log::warn!("couldn't open {}: {e}", program.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_steps_in_tens_and_stays_lit() {
        assert_eq!(step_brightness(50, true), 60);
        assert_eq!(step_brightness(55, true), 60);
        assert_eq!(step_brightness(100, true), 100);
        assert_eq!(step_brightness(50, false), 40);
        assert_eq!(step_brightness(55, false), 50);
        assert_eq!(step_brightness(10, false), 10);
        assert_eq!(step_brightness(0, true), 10);
    }
}
