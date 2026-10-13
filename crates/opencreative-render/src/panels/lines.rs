//! The "Lines" skin, in the style of DeepCool's LM digital layout: one
//! large reading and two smaller ones over a grid of dark tiles, framed by
//! thin interlocking lines with small nodes where they meet.
//!
//! Geometry is defined once in a 320x240 landscape design space. Portrait
//! panels use the same design transposed, so lines still connect.

use opencreative_sensors::{Metric, Snapshot};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::monitor::rgb;

use super::{
    CLAUDE, Layout, accent, crisp_width, fill_rect, level, metric_icon, shows_claude, snap,
    spark_icon,
};
use crate::text::{self, Align, Weight};

const DESIGN_W: f32 = 320.0;
const DESIGN_H: f32 = 240.0;

/// Tile grid: 8 px square tiles with 2 px black gaps (at 320x240).
const TILE: f32 = 8.0;
const TILE_PITCH: f32 = 10.0;
/// Exact RGB565 grays, darkest to brightest, so flat areas never dither.
const TILE_GRAYS: [u8; 4] = [0, 8, 16, 24];
/// The darkest point of the grid, as fractions of the width and height:
/// just left of center, behind the large reading.
const SHADE_CENTER: (f32, f32) = (0.45, 0.52);

const LINE: [u8; 3] = [0xe0, 0xe4, 0xe8];
const FAINT_LINE: [u8; 3] = [0x50, 0x54, 0x58];
const VALUE: [u8; 3] = [0xf8, 0xfc, 0xf8];
const LABEL: [u8; 3] = [0xc8, 0xcc, 0xd0];
const MUTED: [u8; 3] = [0x70, 0x74, 0x78];
const DETAIL: [u8; 3] = [0x98, 0x9c, 0xa0];
const METER_OFF: [u8; 3] = [0x38, 0x3c, 0x40];

/// An axis-aligned box in design space.
#[derive(Debug, Clone, Copy)]
struct Area {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Area {
    const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }
}

/// The three areas the lines frame (design space, landscape).
const PRIMARY: Area = Area::new(16.0, 30.0, 190.0, 212.0);
const TOP: Area = Area::new(203.0, 18.0, 305.0, 120.0);
const BOTTOM: Area = Area::new(203.0, 120.0, 305.0, 212.0);

/// Maps design space to the output, transposing for portrait.
#[derive(Debug, Clone, Copy)]
struct Space {
    scale: f32,
    portrait: bool,
}

impl Space {
    fn new(width: u32, height: u32) -> Self {
        let portrait = height > width;
        let (long, short) = if portrait {
            (height, width)
        } else {
            (width, height)
        };
        let scale = (long as f32 / DESIGN_W).min(short as f32 / DESIGN_H);
        Self { scale, portrait }
    }

    fn point(self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x * self.scale, y * self.scale);
        if self.portrait { (y, x) } else { (x, y) }
    }

    /// An area's output rectangle as (left, top, right, bottom).
    fn area(self, a: Area) -> (f32, f32, f32, f32) {
        let (ax, ay) = self.point(a.x0, a.y0);
        let (bx, by) = self.point(a.x1, a.y1);
        (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by))
    }
}

/// Renders at `width` x `height`.
pub(super) fn render(snap: &Snapshot, layout: &Layout, width: u32, height: u32) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    tiles(&mut pm);
    let space = Space::new(width, height);
    frame(&mut pm, space);
    if shows_claude(layout) {
        claude_badge(&mut pm, space);
    }

    let primary = space.area(PRIMARY);
    reading(
        &mut pm,
        snap,
        layout.primary,
        primary,
        space.scale,
        Size::Large,
    );
    meter(&mut pm, snap, layout.primary, primary, space.scale);
    let top = space.area(TOP);
    reading(
        &mut pm,
        snap,
        layout.secondary[0],
        top,
        space.scale,
        Size::Small,
    );
    let bottom = space.area(BOTTOM);
    reading(
        &mut pm,
        snap,
        layout.secondary[1],
        bottom,
        space.scale,
        Size::SmallLow,
    );
    pm
}

/// Dark square tiles on black, brightest at the edges and fading to black
/// behind the large reading. Each tile is one flat gray; the in-between
/// shades come from an ordered dither across tiles, so the fade looks smooth
/// without any tile dithering on the panel.
fn tiles(pm: &mut Pixmap) {
    const BAYER: [[f32; 4]; 4] = [
        [0.0, 8.0, 2.0, 10.0],
        [12.0, 4.0, 14.0, 6.0],
        [3.0, 11.0, 1.0, 9.0],
        [15.0, 7.0, 13.0, 5.0],
    ];
    pm.fill(rgb([0, 0, 0]));
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let scale = (w.max(h) / DESIGN_W).min(w.min(h) / DESIGN_H);
    let (tile, pitch) = (TILE * scale, TILE_PITCH * scale);
    let (cx, cy) = (w * SHADE_CENTER.0, h * SHADE_CENTER.1);
    let top = TILE_GRAYS.len() as f32 - 1.0;
    // Start half a gap in, so the grid sits evenly in the frame.
    let x0 = ((w % pitch) - (pitch - tile)) / 2.0;
    let y0 = ((h % pitch) - (pitch - tile)) / 2.0;
    let mut row = 0;
    let mut y = y0;
    while y < h {
        let mut col = 0;
        let mut x = x0;
        while x < w {
            // Elliptical distance from the dark spot, 0 there and 1 at the edges.
            let dx = (x + tile / 2.0 - cx) / (w / 2.0);
            let dy = (y + tile / 2.0 - cy) / (h / 2.0);
            let d = (dx * dx + dy * dy).sqrt().min(1.0);
            let level = d.powf(1.3) * top;
            let threshold = (BAYER[row % 4][col % 4] + 0.5) / 16.0;
            let index = (level.floor() + f32::from(level.fract() > threshold)).min(top) as usize;
            let gray = TILE_GRAYS[index];
            if gray > 0 {
                fill_rect(pm, x, y, tile, tile, [gray, gray, gray]);
            }
            x += pitch;
            col += 1;
        }
        y += pitch;
        row += 1;
    }
}

/// The interlocking lines: the primary area's top and right edges, and a
/// second line along its bottom that climbs up to frame the two small areas.
fn frame(pm: &mut Pixmap, space: Space) {
    // Primary: top edge, then down its right side into a node.
    polyline(
        pm,
        space,
        &[(16.0, 30.0), (190.0, 30.0), (190.0, 129.0)],
        LINE,
    );
    node(pm, space, 191.0, 132.0);
    polyline(pm, space, &[(194.0, 133.0), (272.0, 133.0)], FAINT_LINE);
    // Bottom edge, up into a node, then the small areas' shared edge and
    // the top area's right side.
    polyline(
        pm,
        space,
        &[(14.0, 212.0), (203.0, 212.0), (203.0, 123.0)],
        LINE,
    );
    node(pm, space, 205.0, 120.0);
    polyline(
        pm,
        space,
        &[(208.0, 120.0), (305.0, 120.0), (305.0, 18.0)],
        LINE,
    );
}

/// A thin line through design-space points with rounded corners.
fn polyline(pm: &mut Pixmap, space: Space, points: &[(f32, f32)], color: [u8; 3]) {
    let radius = 6.0 * space.scale;
    let width = crisp_width(space.scale);
    let pts: Vec<(f32, f32)> = points
        .iter()
        .map(|&(x, y)| {
            let (x, y) = space.point(x, y);
            (snap(x, width), snap(y, width))
        })
        .collect();
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for i in 1..pts.len() {
        let (x, y) = pts[i];
        if i + 1 < pts.len() {
            // Stop short of the corner and curve into the next segment.
            let into = toward((x, y), pts[i - 1], radius);
            let out = toward((x, y), pts[i + 1], radius);
            pb.line_to(into.0, into.1);
            pb.quad_to(x, y, out.0, out.1);
        } else {
            pb.line_to(x, y);
        }
    }
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(rgb(color));
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// The point `distance` from `from` toward `to`.
fn toward(from: (f32, f32), to: (f32, f32), distance: f32) -> (f32, f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let t = (distance / len).min(0.5);
    (from.0 + dx * t, from.1 + dy * t)
}

/// A small ring where lines meet.
fn node(pm: &mut Pixmap, space: Space, x: f32, y: f32) {
    let width = crisp_width(space.scale);
    let (cx, cy) = space.point(x, y);
    let (cx, cy) = (snap(cx, width), snap(cy, width));
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, 2.6 * space.scale);
    let Some(path) = pb.finish() else { return };
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(rgb([0, 0, 0]));
    pm.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    paint.set_color(rgb(LINE));
    let stroke = Stroke {
        width,
        ..Stroke::default()
    };
    pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

/// Where a reading sits inside its area.
#[derive(Debug, Clone, Copy)]
enum Size {
    Large,
    Small,
    /// The bottom-right reading sits lower in its area, as in the original.
    SmallLow,
}

impl Size {
    /// (value font size in design units, value center and label center as
    /// fractions of the area height). A detail line under the label moves
    /// the low reading up to make room.
    fn metrics(self, detail: bool) -> (f32, f32, f32) {
        match self {
            Size::Large => (70.0, 0.45, 0.71),
            Size::Small => (40.0, 0.42, 0.76),
            Size::SmallLow if detail => (40.0, 0.4, 0.72),
            Size::SmallLow => (40.0, 0.52, 0.88),
        }
    }

    fn inset(self) -> f32 {
        match self {
            Size::Large => 16.0,
            Size::Small | Size::SmallLow => 10.0,
        }
    }
}

/// A value with its unit, and an icon + label underneath.
fn reading(
    pm: &mut Pixmap,
    snap: &Snapshot,
    metric: Metric,
    (left, top, right, bottom): (f32, f32, f32, f32),
    scale: f32,
    size: Size,
) {
    let detail = metric.detail(snap);
    let (value_size, value_at, label_at) = size.metrics(detail.is_some());
    let h = bottom - top;
    let x = left + size.inset() * scale;
    let reading = metric.read(snap);
    let (value, color) = match &reading.value {
        Some(v) => (v.as_str(), VALUE),
        None => ("--", MUTED),
    };

    // Leave room for the level meter beside the large reading.
    let meter_room = if matches!(size, Size::Large) {
        22.0 * scale
    } else {
        0.0
    };
    let max_w = right - x - meter_room - 6.0 * scale;
    let mut v_size = value_size * scale;
    let unit_of = |s: f32| s * 0.3;
    let gap_of = |s: f32| s * 0.12;
    let total = |s: f32| {
        text::measure(value, s, Weight::Display)
            + gap_of(s)
            + text::measure(reading.unit, unit_of(s), Weight::Display)
    };
    if total(v_size) > max_w {
        v_size *= max_w / total(v_size);
    }
    let cy = top + h * value_at;
    text::draw(
        pm,
        value,
        x,
        cy,
        v_size,
        Weight::Display,
        Align::Left,
        color,
    );
    // The unit sits on the value's baseline.
    let value_w = text::measure(value, v_size, Weight::Display);
    text::draw(
        pm,
        reading.unit,
        x + value_w + gap_of(v_size),
        cy + v_size * 0.24,
        unit_of(v_size),
        Weight::Display,
        Align::Left,
        VALUE,
    );

    let mut label_size = if matches!(size, Size::Large) {
        12.0
    } else {
        11.5
    } * scale;
    // Long labels ("WEEKLY LIMIT") shrink to stay on the screen.
    let label_w = |s: f32| s * 1.3 + 6.0 * scale + text::measure(metric.label(), s, Weight::Label);
    let room = right - x - 4.0 * scale;
    if label_w(label_size) > room {
        label_size *= room / label_w(label_size);
    }
    let ly = top + h * label_at;
    let icon = label_size * 1.3;
    metric_icon(pm, metric, x + icon / 2.0 + scale, ly, icon);
    text::draw(
        pm,
        metric.label(),
        x + icon + 6.0 * scale,
        ly,
        label_size,
        Weight::Label,
        Align::Left,
        LABEL,
    );
    if let Some(detail) = detail {
        text::draw(
            pm,
            &detail,
            x + icon + 6.0 * scale,
            ly + label_size * 1.3,
            label_size * 0.82,
            Weight::SemiBold,
            Align::Left,
            DETAIL,
        );
    }
}

/// Claude's mark and name in the free strip above the primary area, when a
/// Claude limit is on screen. Portrait has room for the mark only.
fn claude_badge(pm: &mut Pixmap, space: Space) {
    let (x, y) = space.point(24.0, 15.0);
    let size = 11.0 * space.scale;
    spark_icon(pm, x, y, size, CLAUDE);
    if !space.portrait {
        text::draw(
            pm,
            "Claude",
            x + size * 0.8,
            y,
            11.0 * space.scale,
            Weight::SemiBold,
            Align::Left,
            CLAUDE,
        );
    }
}

/// Five stacked squares beside the large reading, lit from the bottom.
fn meter(
    pm: &mut Pixmap,
    snap: &Snapshot,
    metric: Metric,
    (_, top, right, _): (f32, f32, f32, f32),
    scale: f32,
) {
    const SEGMENTS: usize = 5;
    let lit =
        level(snap, metric).map_or(0, |f| (f.clamp(0.0, 1.0) * SEGMENTS as f32).ceil() as usize);
    let side = 7.0 * scale;
    let pitch = 9.5 * scale;
    let x = right - 10.0 * scale - side;
    let y0 = top + 21.0 * scale;
    for i in 0..SEGMENTS {
        // Index 0 is the top square; light from the bottom up.
        let on = SEGMENTS - i <= lit;
        let color = if on { accent(metric) } else { METER_OFF };
        fill_rect(pm, x, y0 + i as f32 * pitch, side, side, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: Layout = Layout {
        primary: Metric::CpuTemp,
        secondary: [Metric::GpuTemp, Metric::GpuPower],
    };

    #[test]
    fn renders_opaque_frames_in_both_orientations() {
        let snap = Snapshot {
            cpu_temp_c: Some(55.0),
            ..Snapshot::default()
        };
        for (w, h) in [(320, 240), (240, 320)] {
            let pm = render(&snap, &LAYOUT, w, h);
            assert_eq!((pm.width(), pm.height()), (w, h));
            assert!(pm.data().as_chunks::<4>().0.iter().all(|px| px[3] == 255));
        }
    }

    #[test]
    fn tile_grays_are_exact_in_rgb565() {
        for g in TILE_GRAYS {
            assert_eq!((g % 8, g % 4), (0, 0));
        }
    }

    #[test]
    fn meter_lights_by_level() {
        let snap = Snapshot {
            cpu_temp_c: Some(55.0),
            ..Snapshot::default()
        };
        let lit = |f: f32| (f * 5.0).ceil() as usize;
        assert_eq!(lit(level(&snap, Metric::CpuTemp).unwrap()), 3);
        assert_eq!(level(&snap, Metric::GpuTemp), None);
    }

    #[test]
    fn portrait_transposes_the_design() {
        let space = Space::new(240, 320);
        assert_eq!(space.point(16.0, 30.0), (30.0, 16.0));
        assert_eq!(space.area(PRIMARY), (30.0, 16.0, 212.0, 190.0));
    }
}
