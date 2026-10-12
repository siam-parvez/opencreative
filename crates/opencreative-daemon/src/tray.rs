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
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip};
use opencreative_ipc::Request;

use crate::Shared;

/// Brightness steps offered in the menu.
const BRIGHTNESS: [u8; 5] = [10, 25, 50, 75, 100];

/// What the menu shows, read from the daemon when something changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct View {
    pub device: Option<String>,
    pub screen_on: bool,
    pub zen: bool,
    pub brightness: u8,
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
            _ => {}
        }
        let _ = self.shared.handle(request);
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

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let view = &self.view;
        let selected = BRIGHTNESS
            .iter()
            .enumerate()
            .min_by_key(|(_, b)| b.abs_diff(view.brightness))
            .map_or(0, |(i, _)| i);
        vec![
            StandardItem {
                label: "Open OpenCreative".into(),
                activate: Box::new(|_: &mut Self| open_app()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Screen on".into(),
                checked: view.screen_on,
                enabled: view.device.is_some(),
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
                enabled: view.device.is_some(),
                activate: Box::new(|tray: &mut Self| {
                    let on = !tray.view.zen;
                    tray.apply(Request::SetZen { on });
                }),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "Brightness".into(),
                enabled: view.device.is_some(),
                submenu: vec![
                    RadioGroup {
                        selected,
                        select: Box::new(|tray: &mut Self, i| {
                            let percent = BRIGHTNESS[i.min(BRIGHTNESS.len() - 1)];
                            tray.apply(Request::SetBrightness { percent });
                        }),
                        options: BRIGHTNESS
                            .iter()
                            .map(|b| RadioItem {
                                label: format!("{b}%"),
                                ..Default::default()
                            })
                            .collect(),
                    }
                    .into(),
                ],
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
        ]
    }
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
