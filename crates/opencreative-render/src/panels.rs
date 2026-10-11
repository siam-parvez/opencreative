//! The "Panels" screen: one large reading and two smaller ones in outlined
//! boxes over a dot grid. Which readings appear is configurable.

use opencreative_sensors::{Metric, Snapshot};
use tiny_skia::{Paint, Pixmap, Rect, Stroke, Transform};

use crate::monitor::{rgb, temp_color};
use crate::shapes::rounded_rect;
use crate::text::{self, Align, Weight};

/// Colors are exact in RGB565 so flat areas never dither or band.
const BG: [u8; 3] = [0x08, 0x0c, 0x10];
const DOT: [u8; 3] = [0x18, 0x1c, 0x28];
const OUTLINE: [u8; 3] = [0x40, 0x48, 0x58];
const TEXT: [u8; 3] = [0xf8, 0xfc, 0xf8];
const MUTED: [u8; 3] = [0x88, 0x94, 0xa8];
const ACCENT: [u8; 3] = [0x20, 0xd0, 0xe8];

const DOT_SPACING: usize = 6;
const MARGIN: f32 = 8.0;
const GAP: f32 = 8.0;

/// Which readings the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub primary: Metric,
    pub secondary: [Metric; 2],
}

/// Renders at `width` x `height`; portrait sizes stack the boxes vertically.
pub fn render(snap: &Snapshot, layout: &Layout, width: u32, height: u32) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    background(&mut pm);

    let (w, h) = (width as f32, height as f32);
    let [primary, top, bottom] = if w >= h {
        let split = (w * 0.6).round();
        let side_w = w - split - MARGIN - GAP / 2.0;
        let side_h = (h - 2.0 * MARGIN - GAP) / 2.0;
        [
            Panel::new(MARGIN, MARGIN, split - MARGIN - GAP / 2.0, h - 2.0 * MARGIN),
            Panel::new(split + GAP / 2.0, MARGIN, side_w, side_h),
            Panel::new(split + GAP / 2.0, MARGIN + side_h + GAP, side_w, side_h),
        ]
    } else {
        let split = (h * 0.58).round();
        let low_w = (w - 2.0 * MARGIN - GAP) / 2.0;
        let low_h = h - split - MARGIN - GAP / 2.0;
        [
            Panel::new(MARGIN, MARGIN, w - 2.0 * MARGIN, split - MARGIN - GAP / 2.0),
            Panel::new(MARGIN, split + GAP / 2.0, low_w, low_h),
            Panel::new(MARGIN + low_w + GAP, split + GAP / 2.0, low_w, low_h),
        ]
    };

    primary.draw(&mut pm, snap, layout.primary, 0.48);
    top.draw(&mut pm, snap, layout.secondary[0], 0.42);
    bottom.draw(&mut pm, snap, layout.secondary[1], 0.42);
    pm
}

fn background(pm: &mut Pixmap) {
    pm.fill(rgb(BG));
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data_mut();
    for y in (DOT_SPACING / 2..h).step_by(DOT_SPACING) {
        for x in (DOT_SPACING / 2..w).step_by(DOT_SPACING) {
            // 2x2 dots.
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (px, py) = (x + dx, y + dy);
                if px < w && py < h {
                    let i = (py * w + px) * 4;
                    data[i..i + 3].copy_from_slice(&DOT);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Panel {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Panel {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Draws the outline, an accent tick, the value with its unit, and the
    /// label. `scale` is the value's height as a fraction of the panel height.
    fn draw(self, pm: &mut Pixmap, snap: &Snapshot, metric: Metric, scale: f32) {
        let Panel { x, y, w, h } = self;
        if let Some(path) = rounded_rect(x, y, w, h, 6.0) {
            let mut paint = Paint::default();
            paint.set_color(rgb(OUTLINE));
            paint.anti_alias = true;
            let stroke = Stroke {
                width: 1.5,
                ..Stroke::default()
            };
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        let reading = metric.read(snap);
        let accent = match (metric, &reading.value) {
            (Metric::CpuTemp | Metric::GpuTemp, Some(v)) => {
                v.parse().map(temp_color).unwrap_or(ACCENT)
            }
            _ => ACCENT,
        };
        // A short accent bar in the top-left corner.
        let tick_w = (w * 0.16).clamp(12.0, 28.0);
        if let Some(rect) = Rect::from_xywh(x + 10.0, y + 10.0, tick_w, 3.0) {
            let mut paint = Paint::default();
            paint.set_color(rgb(accent));
            pm.fill_rect(rect, &paint, Transform::identity(), None);
        }

        let label_size = (h * 0.11).clamp(10.0, 16.0);
        text::draw(
            pm,
            metric.label(),
            x + w / 2.0,
            y + h - label_size * 1.1,
            label_size,
            Weight::SemiBold,
            Align::Center,
            MUTED,
        );

        let (value, color) = match &reading.value {
            Some(v) => (v.as_str(), TEXT),
            None => ("--", MUTED),
        };
        let max_w = w * 0.86;
        let mut size = h * scale;
        let unit_of = |size: f32| size * 0.3;
        let gap_of = |size: f32| size * 0.06;
        let total = |size: f32| {
            text::measure(value, size, Weight::Medium)
                + gap_of(size)
                + text::measure(reading.unit, unit_of(size), Weight::SemiBold)
        };
        // Shrink long values to fit rather than overflow the panel.
        let width = total(size);
        if width > max_w {
            size *= max_w / width;
        }
        let cy = y + h * 0.46;
        let left = x + (w - total(size)) / 2.0;
        let value_w = text::measure(value, size, Weight::Medium);
        text::draw(
            pm,
            value,
            left,
            cy,
            size,
            Weight::Medium,
            Align::Left,
            color,
        );
        text::draw(
            pm,
            reading.unit,
            left + value_w + gap_of(size),
            cy + size * 0.22,
            unit_of(size),
            Weight::SemiBold,
            Align::Left,
            MUTED,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        primary: Metric::CpuTemp,
        secondary: [Metric::CpuFreq, Metric::CpuPower],
    };

    #[test]
    fn renders_opaque_frames_in_both_orientations() {
        let snap = Snapshot {
            cpu_temp_c: Some(70.0),
            cpu_mhz: 5600,
            cpu_power_w: Some(59.1),
            ..Snapshot::default()
        };
        for (w, h) in [(320, 240), (240, 320)] {
            let pm = render(&snap, &LAYOUT, w, h);
            assert_eq!((pm.width(), pm.height()), (w, h));
            assert!(pm.data().as_chunks::<4>().0.iter().all(|px| px[3] == 255));
        }
    }

    #[test]
    fn background_colors_are_exact_in_rgb565() {
        for c in [BG, DOT] {
            assert_eq!((c[0] % 8, c[1] % 4, c[2] % 8), (0, 0, 0));
        }
    }
}
