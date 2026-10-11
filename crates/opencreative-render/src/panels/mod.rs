//! The "Panels" screen: one large reading and two smaller ones, drawn in a
//! choice of skins. Which readings appear is configurable.

mod hud;
mod lines;

use opencreative_sensors::{Metric, Snapshot};
use tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::monitor::rgb;

const CPU: [u8; 3] = [0x58, 0xd0, 0xe8];
const GPU: [u8; 3] = [0xe8, 0xc0, 0x60];
const RAM: [u8; 3] = [0x70, 0xd8, 0x90];
/// Claude's coral.
const CLAUDE: [u8; 3] = [0xe0, 0x88, 0x60];

/// Moves a point so a `width`-pixel line through it covers whole pixels:
/// odd widths sit on pixel centers, even widths on pixel edges.
fn snap(x: f32, width: f32) -> f32 {
    if (width.round() as i32) % 2 == 1 {
        x.floor() + 0.5
    } else {
        x.round()
    }
}

/// A line width in whole pixels, at least one.
fn crisp_width(width: f32) -> f32 {
    width.round().max(1.0)
}

/// Which readings the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub primary: Metric,
    pub secondary: [Metric; 2],
}

/// How the readings are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skin {
    /// Thin interlocking lines over a tile grid.
    #[default]
    Lines,
    /// Neon frames with live charts.
    Hud,
}

impl Skin {
    pub const ALL: [Skin; 2] = [Skin::Lines, Skin::Hud];
}

/// Renders at `width` x `height`. `history` holds recent readings, oldest
/// first, for skins that chart them.
pub fn render(
    snap: &Snapshot,
    history: &[Snapshot],
    layout: &Layout,
    skin: Skin,
    width: u32,
    height: u32,
) -> Pixmap {
    match skin {
        Skin::Lines => lines::render(snap, layout, width, height),
        Skin::Hud => hud::render(snap, history, layout, width, height),
    }
}

fn accent(metric: Metric) -> [u8; 3] {
    match metric {
        Metric::CpuTemp | Metric::CpuLoad | Metric::CpuFreq | Metric::CpuPower => CPU,
        Metric::GpuTemp | Metric::GpuLoad | Metric::GpuFreq | Metric::GpuPower => GPU,
        Metric::RamUsage | Metric::RamUsed => RAM,
        Metric::ClaudeSession | Metric::ClaudeWeekly => CLAUDE,
    }
}

/// The icon beside a reading's label: Claude's spark for its limits, a
/// chip in the reading's color for everything else.
fn metric_icon(pm: &mut Pixmap, metric: Metric, cx: f32, cy: f32, size: f32) {
    if metric.is_claude() {
        spark_icon(pm, cx, cy, size * 1.1, CLAUDE);
    } else {
        chip_icon(pm, cx, cy, size, accent(metric));
    }
}

/// Whether any shown reading is a Claude limit.
fn shows_claude(layout: &Layout) -> bool {
    layout.primary.is_claude() || layout.secondary.iter().any(|m| m.is_claude())
}

/// Claude's mark: tapered rays of uneven length around a small hub.
fn spark_icon(pm: &mut Pixmap, cx: f32, cy: f32, size: f32, color: [u8; 3]) {
    const RAYS: [f32; 12] = [
        1.0, 0.78, 0.92, 0.7, 0.96, 0.8, 1.0, 0.74, 0.9, 0.82, 0.94, 0.72,
    ];
    let r = size / 2.0;
    let mut pb = PathBuilder::new();
    for (i, len) in RAYS.iter().enumerate() {
        let a = i as f32 * std::f32::consts::TAU / RAYS.len() as f32 - std::f32::consts::FRAC_PI_2;
        let (dx, dy) = (a.cos(), a.sin());
        let w = 0.1 * r;
        let base = 0.12 * r;
        pb.move_to(cx + dx * base - dy * w, cy + dy * base + dx * w);
        pb.line_to(cx + dx * r * len, cy + dy * r * len);
        pb.line_to(cx + dx * base + dy * w, cy + dy * base - dx * w);
        pb.close();
    }
    pb.push_circle(cx, cy, 0.16 * r);
    let Some(path) = pb.finish() else { return };
    let mut paint = Paint::default();
    paint.set_color(rgb(color));
    paint.anti_alias = true;
    pm.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// A processor chip: an outlined square with pins and a solid core.
fn chip_icon(pm: &mut Pixmap, cx: f32, cy: f32, size: f32, color: [u8; 3]) {
    let body = size * 0.62;
    let (x0, y0) = (cx - body / 2.0, cy - body / 2.0);
    let mut paint = Paint::default();
    paint.set_color(rgb(color));
    paint.anti_alias = true;
    let stroke = Stroke {
        width: (size * 0.1).max(1.0),
        ..Stroke::default()
    };
    let mut pb = PathBuilder::new();
    if let Some(rect) = Rect::from_xywh(x0, y0, body, body) {
        pb.push_rect(rect);
    }
    let pin = (size - body) / 2.0;
    for t in [0.3, 0.7] {
        let along = body * t;
        // Pins on all four sides.
        pb.move_to(x0 + along, y0);
        pb.line_to(x0 + along, y0 - pin);
        pb.move_to(x0 + along, y0 + body);
        pb.line_to(x0 + along, y0 + body + pin);
        pb.move_to(x0, y0 + along);
        pb.line_to(x0 - pin, y0 + along);
        pb.move_to(x0 + body, y0 + along);
        pb.line_to(x0 + body + pin, y0 + along);
    }
    if let Some(path) = pb.finish() {
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let core = body * 0.42;
    if let Some(rect) = Rect::from_xywh(cx - core / 2.0, cy - core / 2.0, core, core) {
        pm.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// How full the meter is for a reading, 0..1.
fn level(snap: &Snapshot, metric: Metric) -> Option<f32> {
    let pct = |v: f32| v / 100.0;
    Some(match metric {
        Metric::CpuTemp => pct(snap.cpu_temp_c?),
        Metric::GpuTemp => pct(snap.gpu.temp_c?),
        Metric::CpuLoad => pct(snap.cpu_load_pct),
        Metric::GpuLoad => pct(snap.gpu.load_pct?),
        Metric::RamUsage | Metric::RamUsed => pct(snap.mem_used_pct),
        Metric::CpuFreq => snap.cpu_mhz as f32 / 6000.0,
        Metric::GpuFreq => snap.gpu.mhz? as f32 / 3000.0,
        Metric::CpuPower => snap.cpu_power_w? / 250.0,
        Metric::GpuPower => snap.gpu.power_w? / 450.0,
        Metric::ClaudeSession => pct(snap.claude?.session?.pct),
        Metric::ClaudeWeekly => pct(snap.claude?.weekly?.pct),
    })
}

fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: [u8; 3]) {
    if let Some(rect) = Rect::from_xywh(x, y, w, h) {
        let mut paint = Paint::default();
        paint.set_color(rgb(color));
        pm.fill_rect(rect, &paint, Transform::identity(), None);
    }
}
