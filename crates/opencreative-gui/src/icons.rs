//! Icons: Lucide line icons (ISC, `assets/icons/LICENSE-lucide.txt`),
//! drawn white and tinted per use, plus Claude's mark painted in code.

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{self, Color32, ImageSource, Painter, Rect, Sense, Stroke, Vec2, pos2};

/// Claude's coral, for its mark and usage bars.
pub const CLAUDE: Color32 = Color32::from_rgb(0xd9, 0x77, 0x57);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Dashboard,
    Devices,
    Cooler,
    Settings,
    Cpu,
    Gpu,
    Memory,
    Storage,
    Network,
    Up,
    Down,
    Rotate,
    Help,
    Minimize,
    Maximize,
    Restore,
    Close,
    Upload,
    Claude,
}

impl Icon {
    fn source(self) -> Option<ImageSource<'static>> {
        use egui::include_image as svg;
        Some(match self {
            Icon::Dashboard => svg!("../../../assets/icons/gauge.svg"),
            Icon::Devices => svg!("../../../assets/icons/monitor.svg"),
            Icon::Cooler => svg!("../../../assets/icons/fan.svg"),
            Icon::Settings => svg!("../../../assets/icons/settings.svg"),
            Icon::Cpu => svg!("../../../assets/icons/cpu.svg"),
            Icon::Gpu => svg!("../../../assets/icons/microchip.svg"),
            Icon::Memory => svg!("../../../assets/icons/memory-stick.svg"),
            Icon::Storage => svg!("../../../assets/icons/hard-drive.svg"),
            Icon::Network => svg!("../../../assets/icons/globe.svg"),
            Icon::Up => svg!("../../../assets/icons/arrow-up.svg"),
            Icon::Down => svg!("../../../assets/icons/arrow-down.svg"),
            Icon::Rotate => svg!("../../../assets/icons/rotate-cw.svg"),
            Icon::Help => svg!("../../../assets/icons/circle-help.svg"),
            Icon::Minimize => svg!("../../../assets/icons/minus.svg"),
            Icon::Maximize => svg!("../../../assets/icons/square.svg"),
            Icon::Restore => svg!("../../../assets/icons/copy.svg"),
            Icon::Close => svg!("../../../assets/icons/x.svg"),
            Icon::Upload => svg!("../../../assets/icons/upload.svg"),
            Icon::Claude => return None,
        })
    }
}

/// Allocates a `size` square and draws the icon in it.
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui, rect, icon, color);
    response
}

/// Draws `icon` filling `rect` (a square works best).
pub fn paint(ui: &egui::Ui, rect: Rect, icon: Icon, color: Color32) {
    match icon.source() {
        Some(source) => {
            egui::Image::new(source)
                .tint(color)
                .fit_to_exact_size(rect.size())
                .paint_at(ui, rect);
        }
        None => spark(ui.painter(), rect, color),
    }
}

/// Claude's mark: tapered rays of uneven length around a small hub.
fn spark(painter: &Painter, rect: Rect, color: Color32) {
    const RAYS: [f32; 12] = [
        1.0, 0.78, 0.92, 0.7, 0.96, 0.8, 1.0, 0.74, 0.9, 0.82, 0.94, 0.72,
    ];
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let p = |x: f32, y: f32| pos2(c.x + x * s / 2.0, c.y + y * s / 2.0);
    for (i, len) in RAYS.iter().enumerate() {
        let a = i as f32 * TAU / RAYS.len() as f32 - FRAC_PI_2;
        let (dx, dy) = (a.cos(), a.sin());
        let (nx, ny) = (-dy, dx);
        let w = 0.09;
        painter.add(egui::Shape::convex_polygon(
            vec![
                p(dx * 0.12 + nx * w, dy * 0.12 + ny * w),
                p(dx * len * 0.95, dy * len * 0.95),
                p(dx * 0.12 - nx * w, dy * 0.12 - ny * w),
            ],
            color,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(c, s * 0.1, color);
}
