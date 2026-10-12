//! Music bars: a spectrum of what the computer is playing, drawn as
//! segmented LED-style bars (flat colors, which the RGB565 panel shows
//! without banding).

use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::monitor::rgb;
use crate::text::{self, Align, Weight};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// Bars rising from the bottom.
    #[default]
    Bars,
    /// Bars growing up and down from the middle.
    Mirror,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub style: BarStyle,
    pub color: [u8; 3],
    /// Color each bar by its pitch (bass red through treble violet) instead.
    pub rainbow: bool,
}

/// Unlit segments: a faint track showing each bar's full height.
const TRACK: [u8; 3] = [0x14, 0x16, 0x1a];
const NOTE: [u8; 3] = [0x70, 0x74, 0x78];

/// Draws `levels` (0 to 1, low to high pitch) with their `peaks`. A
/// `message` (e.g. why nothing can be heard) is written across the middle.
pub fn render(
    levels: &[f32],
    peaks: &[f32],
    look: &Look,
    message: Option<&str>,
    width: u32,
    height: u32,
) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    pm.fill(rgb([0, 0, 0]));
    let (w, h) = (width as f32, height as f32);
    let n = levels.len();
    if n > 0 {
        let margin = (w * 0.04).round();
        let gap = (w / n as f32 * 0.25).clamp(1.0, 4.0).round();
        let bar_w = ((w - 2.0 * margin - gap * (n as f32 - 1.0)) / n as f32).floor();
        // Center the row after rounding the bar width down.
        let left = ((w - (bar_w * n as f32 + gap * (n as f32 - 1.0))) / 2.0).round();
        let seg_h = (h / 48.0).max(2.0).round();
        let seg_gap = (seg_h / 2.0).max(1.0).floor();
        let pitch = seg_h + seg_gap;
        let (base, span, mirror) = match look.style {
            BarStyle::Bars => (h - margin, h - 2.0 * margin, false),
            BarStyle::Mirror => ((h / 2.0).round(), h / 2.0 - margin, true),
        };
        let segments = (span / pitch).floor().max(1.0) as usize;
        for (i, &level) in levels.iter().enumerate() {
            let x = left + i as f32 * (bar_w + gap);
            let color = if look.rainbow {
                hue(i as f32 / n as f32 * 0.8)
            } else {
                look.color
            };
            let lit = (level.clamp(0.0, 1.0) * segments as f32).round() as usize;
            let peak = peaks
                .get(i)
                .map(|p| (p.clamp(0.0, 1.0) * segments as f32).round() as usize);
            for s in 0..segments {
                let fill = if s < lit {
                    color
                } else if peak.is_some_and(|p| p > 0 && s + 1 == p) {
                    lighten(color)
                } else {
                    TRACK
                };
                let up = base - (s + 1) as f32 * pitch + seg_gap;
                rect(&mut pm, x, up, bar_w, seg_h, fill);
                if mirror {
                    let down = base + s as f32 * pitch;
                    rect(&mut pm, x, down, bar_w, seg_h, dim(fill));
                }
            }
        }
    }
    if let Some(message) = message {
        text::draw(
            &mut pm,
            message,
            w / 2.0,
            h / 2.0,
            (w.min(h) * 0.07).round(),
            Weight::Display,
            Align::Center,
            NOTE,
        );
    }
    pm
}

fn rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: [u8; 3]) {
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        let mut paint = Paint::default();
        paint.set_color(rgb(color));
        pm.fill_rect(r, &paint, Transform::identity(), None);
    }
}

/// Peak caps: halfway to white.
fn lighten(c: [u8; 3]) -> [u8; 3] {
    c.map(|v| v + (255 - v) / 2)
}

/// The mirrored half: a darker reflection.
fn dim(c: [u8; 3]) -> [u8; 3] {
    if c == TRACK { c } else { c.map(|v| v / 3) }
}

/// A fully saturated color around the color wheel (0 red, 1/3 green, 2/3
/// blue).
fn hue(t: f32) -> [u8; 3] {
    let h = t.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [r, g, b].map(|v: f32| (v * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pm: &Pixmap, x: u32, y: u32) -> [u8; 3] {
        let p = pm.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue()]
    }

    #[test]
    fn a_full_bar_lights_to_the_top_and_an_empty_one_stays_dark() {
        let look = Look {
            style: BarStyle::Bars,
            color: [0, 200, 255],
            rainbow: false,
        };
        let pm = render(&[1.0, 0.0], &[1.0, 0.0], &look, None, 320, 240);
        let lit = (0..240)
            .filter(|&y| pixel(&pm, 80, y) == look.color)
            .count();
        let dark = (0..240)
            .filter(|&y| pixel(&pm, 240, y) == look.color)
            .count();
        assert!(lit >= 140, "{lit}");
        assert_eq!(dark, 0);
    }

    #[test]
    fn mirror_reflects_below_the_middle() {
        let look = Look {
            style: BarStyle::Mirror,
            color: [240, 60, 60],
            rainbow: false,
        };
        let pm = render(&[0.5], &[0.5], &look, None, 320, 240);
        let above = (0..120)
            .filter(|&y| pixel(&pm, 160, y) == look.color)
            .count();
        let below = (120..240)
            .filter(|&y| pixel(&pm, 160, y) == dim(look.color))
            .count();
        assert!(above > 20 && above == below, "{above} vs {below}");
    }

    #[test]
    fn rainbow_runs_from_red() {
        assert_eq!(hue(0.0), [255, 0, 0]);
        assert_eq!(hue(1.0 / 3.0), [0, 255, 0]);
    }
}
