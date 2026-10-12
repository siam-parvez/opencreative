//! The top-bar icon: one color, like the desktop's own status icons. A
//! gauge arc (with a faint track) around "OC", drawn on a 16-unit grid as
//! filled outlines, which is what lets desktops recolor it to match the
//! panel. The same shapes give the bitmaps the tray sends and the SVG the
//! packages install (`opencreative-symbolic.svg`).

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

/// One filled shape on the 16x16 grid, and how opaque it is. A NaN point
/// starts a new contour.
struct Shape {
    outline: Vec<(f32, f32)>,
    opacity: f32,
}

const CENTER: (f32, f32) = (8.0, 8.4);
const RADIUS: f32 = 6.4;
const ARC_WIDTH: f32 = 1.7;
/// The gauge opens at the bottom: 270 degrees from bottom-left round to
/// bottom-right, clockwise (y points down).
const ARC_FROM: f32 = 135.0;
const ARC_TO: f32 = 405.0;
/// Where the filled part of the gauge ends.
const ARC_VALUE: f32 = 335.0;
const TRACK_OPACITY: f32 = 0.35;
const LETTER_WIDTH: f32 = 1.3;
const LETTER_RX: f32 = 1.5;
const LETTER_RY: f32 = 2.0;
const LETTER_Y: f32 = 8.9;

fn shapes() -> Vec<Shape> {
    let (cx, cy) = CENTER;
    let ring = |from, to| arc(cx, cy, RADIUS, RADIUS, from, to);
    vec![
        Shape {
            outline: stroke(&ring(ARC_VALUE, ARC_TO), ARC_WIDTH),
            opacity: TRACK_OPACITY,
        },
        Shape {
            outline: stroke(&ring(ARC_FROM, ARC_VALUE), ARC_WIDTH),
            opacity: 1.0,
        },
        Shape {
            // "O": a closed ring.
            outline: ring_outline(6.05, LETTER_Y, LETTER_RX, LETTER_RY, LETTER_WIDTH),
            opacity: 1.0,
        },
        Shape {
            // "C": open on the right.
            outline: stroke(
                &arc(9.95, LETTER_Y, LETTER_RX, LETTER_RY, 45.0, 315.0),
                LETTER_WIDTH,
            ),
            opacity: 1.0,
        },
    ]
}

/// Points along an elliptical arc, `from` to `to` degrees.
fn arc(cx: f32, cy: f32, rx: f32, ry: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    let steps = ((to - from).abs() / 3.0).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = (from + (to - from) * i as f32 / steps as f32).to_radians();
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

/// The unit normal at point `i` of a polyline.
fn normal(points: &[(f32, f32)], i: usize) -> (f32, f32) {
    let a = points[i.saturating_sub(1)];
    let b = points[(i + 1).min(points.len() - 1)];
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    (-dy / len, dx / len)
}

/// The outline of a line `width` wide along `points`, with round ends.
fn stroke(points: &[(f32, f32)], width: f32) -> Vec<(f32, f32)> {
    let h = width / 2.0;
    let side = |sign: f32| -> Vec<(f32, f32)> {
        (0..points.len())
            .map(|i| {
                let (nx, ny) = normal(points, i);
                (points[i].0 + sign * nx * h, points[i].1 + sign * ny * h)
            })
            .collect()
    };
    // Half a circle round `at`, from the `from` side to the other.
    let cap = |at: (f32, f32), from: (f32, f32)| -> Vec<(f32, f32)> {
        let start = from.1.atan2(from.0);
        (1..12)
            .map(|k| {
                let a = start - std::f32::consts::PI * k as f32 / 12.0;
                (at.0 + h * a.cos(), at.1 + h * a.sin())
            })
            .collect()
    };
    let last = points.len() - 1;
    let mut out = side(1.0);
    out.extend(cap(points[last], normal(points, last)));
    let mut back = side(-1.0);
    back.reverse();
    out.extend(back);
    let n = normal(points, 0);
    out.extend(cap(points[0], (-n.0, -n.1)));
    out
}

/// An elliptical ring `width` thick: outer edge, then (as a second contour)
/// the inner edge, so an even-odd fill leaves the middle open.
fn ring_outline(cx: f32, cy: f32, rx: f32, ry: f32, width: f32) -> Vec<(f32, f32)> {
    let h = width / 2.0;
    let mut out = arc(cx, cy, rx + h, ry + h, 0.0, 360.0);
    out.push((f32::NAN, f32::NAN));
    let mut inner = arc(cx, cy, rx - h, ry - h, 0.0, 360.0);
    inner.reverse();
    out.extend(inner);
    out
}

/// The contours of a shape, split at NaN points.
fn contours(outline: &[(f32, f32)]) -> impl Iterator<Item = &[(f32, f32)]> {
    outline.split(|p| p.0.is_nan()).filter(|c| !c.is_empty())
}

/// The icon as `size` x `size` pixels in `rgb`.
pub fn render(size: u32, rgb: [u8; 3]) -> Pixmap {
    let mut pm = Pixmap::new(size, size).expect("non-zero size");
    let scale = size as f32 / 16.0;
    for shape in shapes() {
        let mut path = PathBuilder::new();
        for contour in contours(&shape.outline) {
            path.move_to(contour[0].0 * scale, contour[0].1 * scale);
            for &(x, y) in &contour[1..] {
                path.line_to(x * scale, y * scale);
            }
            path.close();
        }
        let Some(path) = path.finish() else {
            continue;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            rgb[0],
            rgb[1],
            rgb[2],
            (shape.opacity * 255.0).round() as u8,
        );
        paint.anti_alias = true;
        pm.fill_path(
            &path,
            &paint,
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
    }
    pm
}

/// The icon as a symbolic SVG: desktops recolor its fill to match the panel.
/// Written to `packaging/linux/opencreative-symbolic.svg` by the tests.
#[cfg(test)]
pub fn svg() -> String {
    let mut out = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" \
         viewBox=\"0 0 16 16\">\n",
    );
    for shape in shapes() {
        let mut d = String::new();
        for contour in contours(&shape.outline) {
            for (i, (x, y)) in contour.iter().enumerate() {
                d.push_str(&format!("{}{x:.2} {y:.2}", if i == 0 { "M" } else { "L" }));
            }
            d.push('Z');
        }
        let opacity = if shape.opacity < 1.0 {
            format!(" fill-opacity=\"{}\"", shape.opacity)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  <path fill=\"#bebebe\"{opacity} fill-rule=\"evenodd\" d=\"{d}\"/>\n"
        ));
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALLED: &str = include_str!("../../../packaging/linux/opencreative-symbolic.svg");

    /// Rewrites the installed SVG when `OPENCREATIVE_WRITE_ICON` is set,
    /// else checks it matches the drawing.
    #[test]
    fn installed_svg_matches_the_drawing() {
        if std::env::var_os("OPENCREATIVE_WRITE_ICON").is_some() {
            std::fs::write(
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../packaging/linux/opencreative-symbolic.svg"
                ),
                svg(),
            )
            .unwrap();
            return;
        }
        assert!(
            INSTALLED == svg(),
            "regenerate it: OPENCREATIVE_WRITE_ICON=1 cargo test -p opencreative-daemon tray_icon"
        );
    }

    /// Writes PNG previews to `OPENCREATIVE_ICON_PREVIEW` (a folder) when set.
    #[test]
    fn previews_when_asked() {
        let Some(dir) = std::env::var_os("OPENCREATIVE_ICON_PREVIEW") else {
            return;
        };
        for size in [16, 22, 32, 128] {
            let path = std::path::Path::new(&dir).join(format!("tray-{size}.png"));
            render(size, [255, 255, 255]).save_png(path).unwrap();
        }
    }

    #[test]
    fn one_color_on_nothing() {
        let pm = render(64, [255, 255, 255]);
        assert!(pm.pixels().iter().any(|p| p.alpha() == 255));
        // White only (premultiplied: each channel equals alpha).
        assert!(
            pm.pixels()
                .iter()
                .all(|p| p.red() == p.alpha() && p.green() == p.alpha() && p.blue() == p.alpha())
        );
        // No background box: the corners are empty.
        assert_eq!(pm.pixel(2, 2).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(61, 61).unwrap().alpha(), 0);
    }
}
