//! The default "system monitor" screen: CPU temperature and load gauges with a
//! status strip for GPU temperature, RAM and the clock.

use std::f32::consts::PI;

use opencreative_sensors::Snapshot;
use tiny_skia::{
    Color, GradientStop, LineCap, LinearGradient, Paint, PathBuilder, Pixmap, Point, Rect,
    SpreadMode, Stroke, Transform,
};

use crate::text::{self, Align, Weight};

const BG_TOP: [u8; 3] = [0x0b, 0x0f, 0x17];
const BG_BOTTOM: [u8; 3] = [0x13, 0x1a, 0x2a];
const TRACK: [u8; 3] = [0x24, 0x2c, 0x3d];
const TEXT: [u8; 3] = [0xf1, 0xf5, 0xf9];
const MUTED: [u8; 3] = [0x8b, 0x95, 0xa7];

/// Gauge arc: 270 degrees, open at the bottom.
const ARC_START: f32 = 0.75 * PI;
const ARC_SWEEP: f32 = 1.5 * PI;

/// Height of the bottom status strip in pixels.
const STRIP: f32 = 52.0;

/// Renders the monitor screen at `width` x `height`. Portrait sizes stack the
/// gauges vertically.
pub fn render(snap: &Snapshot, clock: &str, width: u32, height: u32) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    let (w, h) = (width as f32, height as f32);
    background(&mut pm, w, h);

    let gauge_area = h - STRIP;
    let cells = if w >= h {
        [
            (w * 0.25, gauge_area / 2.0, w / 2.0, gauge_area),
            (w * 0.75, gauge_area / 2.0, w / 2.0, gauge_area),
        ]
    } else {
        [
            (w / 2.0, gauge_area * 0.25, w, gauge_area / 2.0),
            (w / 2.0, gauge_area * 0.75, w, gauge_area / 2.0),
        ]
    };

    let temp = match snap.cpu_temp_c {
        Some(t) => Gauge {
            fraction: t / 100.0,
            value: format!("{t:.0}°"),
            label: "CPU TEMP",
            color: temp_color(t),
        },
        None => Gauge {
            fraction: 0.0,
            value: "--".to_owned(),
            label: "CPU TEMP",
            color: MUTED,
        },
    };
    let load = snap.cpu_load_pct;
    let load = Gauge {
        fraction: load / 100.0,
        value: format!("{load:.0}%"),
        label: "CPU LOAD",
        color: load_color(load),
    };
    for (gauge, cell) in [temp, load].iter().zip(cells) {
        gauge.draw(&mut pm, cell);
    }

    status_strip(&mut pm, snap, clock, w, h);
    pm
}

fn background(pm: &mut Pixmap, w: f32, h: f32) {
    let shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.0, h),
        vec![
            GradientStop::new(0.0, rgb(BG_TOP)),
            GradientStop::new(1.0, rgb(BG_BOTTOM)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("valid gradient");
    let paint = Paint {
        shader,
        ..Paint::default()
    };
    pm.fill_rect(
        Rect::from_xywh(0.0, 0.0, w, h).expect("valid rect"),
        &paint,
        Transform::identity(),
        None,
    );
}

struct Gauge {
    fraction: f32,
    value: String,
    label: &'static str,
    color: [u8; 3],
}

impl Gauge {
    /// Draws centered at `(cx, cy)` inside a `cell_w` x `cell_h` cell.
    fn draw(&self, pm: &mut Pixmap, (cx, cy, cell_w, cell_h): (f32, f32, f32, f32)) {
        let radius = (cell_w.min(cell_h) / 2.0 - 14.0).max(10.0);
        let thickness = (radius * 0.18).max(4.0);
        arc(pm, cx, cy, radius, 1.0, thickness, TRACK);
        arc(
            pm,
            cx,
            cy,
            radius,
            self.fraction.clamp(0.0, 1.0),
            thickness,
            self.color,
        );

        let value_size = radius * 0.75;
        let label_size = radius * 0.28;
        text::draw(
            pm,
            &self.value,
            cx,
            cy - radius * 0.05,
            value_size,
            Weight::Bold,
            Align::Center,
            TEXT,
        );
        text::draw(
            pm,
            self.label,
            cx,
            cy + radius * 0.82,
            label_size,
            Weight::SemiBold,
            Align::Center,
            MUTED,
        );
    }
}

fn arc(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, fraction: f32, width: f32, color: [u8; 3]) {
    let sweep = ARC_SWEEP * fraction;
    if sweep <= 0.0 {
        return;
    }
    let steps = ((sweep * r) / 3.0).ceil().max(2.0) as u32;
    let mut pb = PathBuilder::new();
    for i in 0..=steps {
        let a = ARC_START + sweep * i as f32 / steps as f32;
        let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    let Some(path) = pb.finish() else { return };
    let mut paint = Paint::default();
    paint.set_color(rgb(color));
    paint.anti_alias = true;
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

fn status_strip(pm: &mut Pixmap, snap: &Snapshot, clock: &str, w: f32, h: f32) {
    let top = h - STRIP;
    let mut paint = Paint::default();
    paint.set_color(rgb(TRACK));
    if let Some(rect) = Rect::from_xywh(12.0, top, w - 24.0, 1.0) {
        pm.fill_rect(rect, &paint, Transform::identity(), None);
    }

    let cy = top + STRIP / 2.0;
    let size = 22.0;
    let (gpu, gpu_color) = match snap.gpu_temp_c {
        Some(t) => (format!("GPU {t:.0}°"), temp_color(t)),
        None => ("GPU --".to_owned(), MUTED),
    };
    let ram = format!("RAM {:.0}%", snap.mem_used_pct);
    text::draw(
        pm,
        &gpu,
        16.0,
        cy,
        size,
        Weight::SemiBold,
        Align::Left,
        gpu_color,
    );
    text::draw(
        pm,
        &ram,
        w / 2.0,
        cy,
        size,
        Weight::SemiBold,
        Align::Center,
        TEXT,
    );
    text::draw(
        pm,
        clock,
        w - 16.0,
        cy,
        size,
        Weight::SemiBold,
        Align::Right,
        TEXT,
    );
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], 255)
}

/// Piecewise-linear color ramp.
fn ramp(stops: &[(f32, [u8; 3])], v: f32) -> [u8; 3] {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if v <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let ((v0, c0), (v1, c1)) = (pair[0], pair[1]);
        if v <= v1 {
            let t = (v - v0) / (v1 - v0);
            return std::array::from_fn(|i| {
                (f32::from(c0[i]) + (f32::from(c1[i]) - f32::from(c0[i])) * t).round() as u8
            });
        }
    }
    last.1
}

fn temp_color(celsius: f32) -> [u8; 3] {
    ramp(
        &[
            (35.0, [0x22, 0xd3, 0xee]),
            (55.0, [0x4a, 0xde, 0x80]),
            (72.0, [0xfb, 0xbf, 0x24]),
            (88.0, [0xef, 0x44, 0x44]),
        ],
        celsius,
    )
}

fn load_color(pct: f32) -> [u8; 3] {
    ramp(
        &[(0.0, [0xa7, 0x8b, 0xfa]), (100.0, [0xf4, 0x72, 0xb6])],
        pct,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_endpoints_and_midpoint() {
        let stops = [(0.0, [0, 0, 0]), (10.0, [100, 200, 50])];
        assert_eq!(ramp(&stops, -5.0), [0, 0, 0]);
        assert_eq!(ramp(&stops, 5.0), [50, 100, 25]);
        assert_eq!(ramp(&stops, 50.0), [100, 200, 50]);
    }

    #[test]
    fn renders_opaque_frames_in_both_orientations() {
        let snap = Snapshot {
            cpu_temp_c: Some(61.0),
            cpu_load_pct: 37.0,
            cpu_mhz: 4200,
            gpu_temp_c: None,
            mem_used_pct: 48.0,
        };
        for (w, h) in [(320, 240), (240, 320)] {
            let pm = render(&snap, "12:34", w, h);
            assert_eq!((pm.width(), pm.height()), (w, h));
            assert!(pm.data().as_chunks::<4>().0.iter().all(|px| px[3] == 255));
        }
    }
}
