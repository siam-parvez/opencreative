//! The "HUD" skin: angular neon frames fading cyan to yellow around the main
//! reading, network speeds and the second reading with live charts.

use opencreative_sensors::{Metric, Snapshot};
use tiny_skia::{
    Color, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Shader, SpreadMode,
    Stroke, Transform,
};

use super::{Layout, accent, chip_icon, fill_rect, level};
use crate::monitor::rgb;
use crate::text::{self, Align, Weight};

/// Exact in RGB565, so the flat background never dithers.
const BG: [u8; 3] = [24, 28, 32];
const CYAN: [u8; 3] = [0x30, 0xe8, 0xf0];
const YELLOW: [u8; 3] = [0xe8, 0xf0, 0x30];
const GLOW: [u8; 3] = [0xa0, 0xd8, 0x40];
const GRID: [u8; 3] = [0x20, 0x40, 0x44];
const TEXT: [u8; 3] = [0xf8, 0xfc, 0xf8];
const MUTED: [u8; 3] = [0x90, 0x98, 0xa0];
const MARGIN: f32 = 8.0;

/// A rectangle in output pixels.
#[derive(Debug, Clone, Copy)]
struct Area {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Area {
    fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    fn w(self) -> f32 {
        self.x1 - self.x0
    }

    fn h(self) -> f32 {
        self.y1 - self.y0
    }

    /// The point at fractions of the width and height.
    fn at(self, fx: f32, fy: f32) -> (f32, f32) {
        (self.x0 + self.w() * fx, self.y0 + self.h() * fy)
    }
}

pub(super) fn render(
    snap: &Snapshot,
    history: &[Snapshot],
    layout: &Layout,
    width: u32,
    height: u32,
) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    pm.fill(rgb(BG));
    let (w, h) = (width as f32, height as f32);

    if w >= h {
        let split = (w * 0.61).round();
        let main = Area::new(MARGIN, MARGIN, split - 4.0, h - MARGIN);
        let net = Area::new(split + 6.0, MARGIN, w - MARGIN, h * 0.36);
        let gap = Area::new(split + 6.0, h * 0.36, w - MARGIN, h * 0.45);
        let side = Area::new(split + 6.0, h * 0.45, w - MARGIN, h - MARGIN);
        main_block(&mut pm, snap, layout.primary, main);
        network(&mut pm, snap, history, net);
        separator(&mut pm, gap);
        secondary(&mut pm, snap, history, layout.secondary[0], side);
    } else {
        let split = (h * 0.62).round();
        let main = Area::new(MARGIN, MARGIN, w - MARGIN, split - 4.0);
        let net = Area::new(MARGIN, split + 6.0, w / 2.0 - 4.0, h - MARGIN);
        let side = Area::new(w / 2.0 + 4.0, split + 6.0, w - MARGIN, h - MARGIN);
        main_block(&mut pm, snap, layout.primary, main);
        network(&mut pm, snap, history, net);
        secondary(&mut pm, snap, history, layout.secondary[0], side);
    }
    pm
}

fn solid(color: [u8; 3]) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(rgb(color));
    paint
}

/// Cyan at `top` fading to yellow at `bottom`.
fn neon(top: f32, bottom: f32) -> Paint<'static> {
    let shader = LinearGradient::new(
        Point::from_xy(0.0, top),
        Point::from_xy(0.0, bottom),
        vec![
            GradientStop::new(0.0, rgb(CYAN)),
            GradientStop::new(1.0, rgb(YELLOW)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    );
    match shader {
        Some(shader) => Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        },
        None => solid(CYAN),
    }
}

fn stroke(pm: &mut Pixmap, points: &[(f32, f32)], close: bool, width: f32, paint: &Paint) {
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        pb.line_to(x, y);
    }
    if close {
        pb.close();
    }
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width,
            ..Stroke::default()
        };
        pm.stroke_path(&path, paint, &stroke, Transform::identity(), None);
    }
}

fn fill(pm: &mut Pixmap, points: &[(f32, f32)], paint: &Paint) {
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        pb.line_to(x, y);
    }
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(
            &path,
            paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// The main reading inside layered neon frames.
fn main_block(pm: &mut Pixmap, snap: &Snapshot, metric: Metric, r: Area) {
    let paint = neon(r.y0, r.y1);
    let line = (r.h() * 0.009).max(1.3);
    let p = |fx: f32, fy: f32| r.at(fx, fy);

    // Outer frame: left side, a chamfered top-left corner and a step.
    stroke(
        pm,
        &[
            p(0.0, 1.0),
            p(0.0, 0.1),
            p(0.08, 0.0),
            p(0.6, 0.0),
            p(0.64, 0.035),
            p(1.0, 0.035),
        ],
        false,
        line,
        &paint,
    );

    // Inner panel with chamfered top corners.
    let (ix0, iy0) = p(0.07, 0.14);
    let (ix1, iy1) = p(0.97, 0.99);
    let c = r.w() * 0.06;
    stroke(
        pm,
        &[
            (ix0, iy0 + c),
            (ix0 + c, iy0),
            (ix1 - c, iy0),
            (ix1, iy0 + c),
            (ix1, iy1),
            (ix0, iy1),
        ],
        true,
        line,
        &paint,
    );

    // Diagonal stripes between the frames; the last two are filled.
    let (sx0, sy0) = p(0.7, 0.06);
    let (sx1, sy1) = p(0.96, 0.105);
    let count = 5;
    let gap = r.w() * 0.012;
    let sw = (sx1 - sx0 - gap * (count - 1) as f32) / count as f32;
    let skew = (sy1 - sy0) * 0.8;
    for i in 0..count {
        let x = sx0 + i as f32 * (sw + gap);
        let shape = [
            (x + skew, sy0),
            (x + sw + skew, sy0),
            (x + sw, sy1),
            (x, sy1),
        ];
        if i >= count - 2 {
            fill(pm, &shape, &solid(YELLOW));
        } else {
            stroke(pm, &shape, true, 1.0, &paint);
        }
    }

    let (icon_x, icon_y) = p(0.36, 0.075);
    chip_icon(pm, icon_x, icon_y, r.h() * 0.075, CYAN);

    // A small bracket inside the panel's top-left corner.
    let b = r.h() * 0.05;
    stroke(
        pm,
        &[
            (ix0 + c * 0.7, iy0 + c + b * 1.2),
            (ix0 + c * 0.7, iy0 + c * 0.7 + b * 0.2),
            (ix0 + c * 0.7 + b * 0.5, iy0 + c * 0.7 - b * 0.3),
            (ix0 + c * 0.7 + b * 1.4, iy0 + c * 0.7 - b * 0.3),
        ],
        false,
        line,
        &solid(YELLOW),
    );

    // A trapezoid and a bar in the bottom-right corner.
    stroke(
        pm,
        &[p(0.74, 0.86), p(0.97, 0.86), p(0.97, 0.93), p(0.71, 0.93)],
        true,
        line,
        &solid(CYAN),
    );
    let (bx, by) = p(0.86, 0.68);
    fill_rect(pm, bx, by, r.w() * 0.018, r.h() * 0.16, YELLOW);

    // Label, value and unit, centered in the panel.
    let cx = (ix0 + ix1) / 2.0;
    let reading = metric.read(snap);
    text::draw(
        pm,
        metric.label(),
        cx,
        p(0.0, 0.27).1,
        r.h() * 0.075,
        Weight::Display,
        Align::Center,
        TEXT,
    );
    let (value, color) = match &reading.value {
        Some(v) => (v.as_str(), TEXT),
        None => ("--", MUTED),
    };
    let max_w = (ix1 - ix0) * 0.85;
    let mut size = r.h() * 0.34;
    let width = text::measure(value, size, Weight::Display);
    if width > max_w {
        size *= max_w / width;
    }
    text::draw(
        pm,
        value,
        cx,
        p(0.0, 0.55).1,
        size,
        Weight::Display,
        Align::Center,
        color,
    );
    text::draw(
        pm,
        reading.unit,
        cx,
        p(0.0, 0.79).1,
        r.h() * 0.09,
        Weight::Display,
        Align::Center,
        TEXT,
    );
}

/// Upload and download speeds over a chart of recent traffic.
fn network(pm: &mut Pixmap, snap: &Snapshot, history: &[Snapshot], r: Area) {
    for (i, (up, bps)) in [(true, snap.net_tx_bps), (false, snap.net_rx_bps)]
        .into_iter()
        .enumerate()
    {
        let y = r.y0 + r.h() * (0.13 + i as f32 * 0.25);
        let icon = r.h() * 0.13;
        tray_arrow(pm, r.x0 + r.w() * 0.09, y, icon, up);
        let (value, unit) = rate_parts(bps);
        let x = r.x0 + r.w() * 0.2;
        let size = r.h() * 0.19;
        text::draw(pm, &value, x, y, size, Weight::Display, Align::Left, TEXT);
        let value_w = text::measure(&value, size, Weight::Display);
        text::draw(
            pm,
            unit,
            x + value_w + size * 0.2,
            y + size * 0.15,
            size * 0.5,
            Weight::Display,
            Align::Left,
            TEXT,
        );
    }

    let peak = history
        .iter()
        .flat_map(|s| [s.net_rx_bps, s.net_tx_bps])
        .fold(1.0_f64, f64::max);
    let scaled = |f: fn(&Snapshot) -> f64| -> Vec<Option<f32>> {
        history.iter().map(|s| Some((f(s) / peak) as f32)).collect()
    };
    let area = Area::new(r.x0 + r.w() * 0.05, r.y0 + r.h() * 0.58, r.x1, r.y1);
    chart(
        pm,
        area,
        &[
            (scaled(|s| s.net_rx_bps), CYAN),
            (scaled(|s| s.net_tx_bps), MUTED),
        ],
    );
}

/// The second reading with a chart of its last minute.
fn secondary(pm: &mut Pixmap, snap: &Snapshot, history: &[Snapshot], metric: Metric, r: Area) {
    let cx = (r.x0 + r.x1) / 2.0;
    let label_size = r.h() * 0.085;
    let label_w = text::measure(metric.label(), label_size, Weight::SemiBold);
    let icon = label_size * 1.1;
    let start = cx - (icon + 5.0 + label_w) / 2.0;
    let ly = r.y0 + r.h() * 0.08;
    chip_icon(pm, start + icon / 2.0, ly, icon, accent(metric));
    text::draw(
        pm,
        metric.label(),
        start + icon + 5.0,
        ly,
        label_size,
        Weight::SemiBold,
        Align::Left,
        TEXT,
    );

    let reading = metric.read(snap);
    let (value, color) = match &reading.value {
        Some(v) => (v.as_str(), TEXT),
        None => ("--", MUTED),
    };
    text::draw(
        pm,
        value,
        cx,
        r.y0 + r.h() * 0.3,
        r.h() * 0.22,
        Weight::Display,
        Align::Center,
        color,
    );
    text::draw(
        pm,
        reading.unit,
        cx,
        r.y0 + r.h() * 0.49,
        r.h() * 0.075,
        Weight::Display,
        Align::Center,
        TEXT,
    );

    let series: Vec<Option<f32>> = history.iter().map(|s| level(s, metric)).collect();
    let area = Area::new(
        r.x0 + r.w() * 0.05,
        r.y0 + r.h() * 0.6,
        r.x1,
        r.y0 + r.h() * 0.88,
    );
    chart(pm, area, &[(series, CYAN)]);
    let ty = r.y0 + r.h() * 0.95;
    let size = r.h() * 0.07;
    text::draw(
        pm,
        "-60s",
        area.x0,
        ty,
        size,
        Weight::SemiBold,
        Align::Left,
        TEXT,
    );
    text::draw(
        pm,
        "Now",
        area.x1,
        ty,
        size,
        Weight::SemiBold,
        Align::Right,
        TEXT,
    );
}

/// Brackets above and below the gap, with a glowing line between them.
fn separator(pm: &mut Pixmap, r: Area) {
    let d = r.h() * 0.25;
    let paint = solid(CYAN);
    stroke(
        pm,
        &[
            (r.x0, r.y0),
            (r.x0 + d, r.y0 + d),
            (r.x1 - d, r.y0 + d),
            (r.x1, r.y0),
        ],
        false,
        1.3,
        &paint,
    );
    stroke(
        pm,
        &[
            (r.x0, r.y1),
            (r.x0 + d, r.y1 - d),
            (r.x1 - d, r.y1 - d),
            (r.x1, r.y1),
        ],
        false,
        1.3,
        &paint,
    );
    let y = (r.y0 + r.y1) / 2.0;
    let shader = LinearGradient::new(
        Point::from_xy(r.x0, y),
        Point::from_xy(r.x1, y),
        vec![
            GradientStop::new(0.0, rgb(BG)),
            GradientStop::new(0.5, rgb(GLOW)),
            GradientStop::new(1.0, rgb(BG)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or(Shader::SolidColor(Color::from_rgba8(
        GLOW[0], GLOW[1], GLOW[2], 255,
    )));
    let glow = Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    };
    stroke(pm, &[(r.x0 + d, y), (r.x1 - d, y)], false, 2.5, &glow);
}

/// A grid with one line per series; values are 0..1, newest at the right.
fn chart(pm: &mut Pixmap, r: Area, series: &[(Vec<Option<f32>>, [u8; 3])]) {
    const SLOTS: usize = 60;
    let grid = solid(GRID);
    for i in 0..=12 {
        let x = r.x0 + r.w() * i as f32 / 12.0;
        stroke(pm, &[(x, r.y0), (x, r.y1)], false, 1.0, &grid);
    }
    for i in 0..=4 {
        let y = r.y0 + r.h() * i as f32 / 4.0;
        stroke(pm, &[(r.x0, y), (r.x1, y)], false, 1.0, &grid);
    }
    for (values, color) in series {
        let recent = &values[values.len().saturating_sub(SLOTS)..];
        let offset = SLOTS - recent.len();
        let dx = r.w() / (SLOTS - 1) as f32;
        let mut run: Vec<(f32, f32)> = Vec::new();
        let paint = solid(*color);
        for (i, v) in recent.iter().enumerate() {
            match v {
                Some(v) => run.push((
                    r.x0 + (offset + i) as f32 * dx,
                    r.y1 - v.clamp(0.0, 1.0) * r.h(),
                )),
                None => {
                    if run.len() > 1 {
                        stroke(pm, &run, false, 1.3, &paint);
                    }
                    run.clear();
                }
            }
        }
        if run.len() > 1 {
            stroke(pm, &run, false, 1.3, &paint);
        }
    }
}

/// An arrow leaving (up) or entering (down) an open tray.
fn tray_arrow(pm: &mut Pixmap, cx: f32, cy: f32, size: f32, up: bool) {
    let paint = solid(TEXT);
    let half = size / 2.0;
    let width = (size * 0.1).max(1.0);
    stroke(
        pm,
        &[
            (cx - half, cy),
            (cx - half, cy + half),
            (cx + half, cy + half),
            (cx + half, cy),
        ],
        false,
        width,
        &paint,
    );
    let (tail, tip) = if up {
        (cy + half * 0.4, cy - half)
    } else {
        (cy - half, cy + half * 0.4)
    };
    stroke(pm, &[(cx, tail), (cx, tip)], false, width, &paint);
    let head = half * 0.5;
    let dir = if up { 1.0 } else { -1.0 };
    stroke(
        pm,
        &[
            (cx - head, tip + head * dir),
            (cx, tip),
            (cx + head, tip + head * dir),
        ],
        false,
        width,
        &paint,
    );
}

/// A rate as a short number and unit, e.g. ("162", "MB/S").
fn rate_parts(bytes_per_sec: f64) -> (String, &'static str) {
    const UNITS: [&str; 4] = ["B/S", "KB/S", "MB/S", "GB/S"];
    let mut v = bytes_per_sec.max(0.0);
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    let value = if v < 10.0 && unit > 0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    };
    (value, UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        primary: Metric::CpuTemp,
        secondary: [Metric::GpuLoad, Metric::RamUsage],
    };

    #[test]
    fn renders_opaque_frames_in_both_orientations() {
        let snap = Snapshot {
            cpu_temp_c: Some(27.0),
            ..Snapshot::default()
        };
        let history = vec![snap.clone(); 30];
        for (w, h) in [(320, 240), (240, 320)] {
            let pm = render(&snap, &history, &LAYOUT, w, h);
            assert_eq!((pm.width(), pm.height()), (w, h));
            assert!(pm.data().as_chunks::<4>().0.iter().all(|px| px[3] == 255));
        }
    }

    #[test]
    fn rates_are_short() {
        assert_eq!(rate_parts(512.0), ("512".to_owned(), "B/S"));
        assert_eq!(
            rate_parts(25.0 * 1024.0 * 1024.0),
            ("25".to_owned(), "MB/S")
        );
        assert_eq!(rate_parts(1.5 * 1024.0), ("1.5".to_owned(), "KB/S"));
    }

    #[test]
    fn background_is_exact_in_rgb565() {
        assert_eq!((BG[0] % 8, BG[1] % 4, BG[2] % 8), (0, 0, 0));
    }
}
