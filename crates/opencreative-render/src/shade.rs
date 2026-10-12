//! Light and dark versions of the screens. Everything is drawn dark; the
//! light version flips each pixel's lightness and keeps its hue, so black
//! backgrounds turn near-white, white text near-black, and accents become
//! deeper shades of the same color, readable on the light background. The
//! mapping is smooth, so anti-aliased edges stay clean.

use tiny_skia::{ColorU8, Pixmap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shade {
    #[default]
    Dark,
    Light,
}

/// Lightness of black once flipped: off-white is easier on the eyes.
const LIGHTEST: f32 = 0.96;
/// Lightness of white once flipped.
const DARKEST: f32 = 0.06;
/// How much darker a fully saturated color goes than a grey would.
const ACCENT_DEPTH: f32 = 0.14;

/// Turns a dark screen light (in place); `Dark` leaves it alone.
pub fn apply(pm: &mut Pixmap, shade: Shade) {
    if shade == Shade::Dark {
        return;
    }
    for px in pm.pixels_mut() {
        // Screens are opaque, so premultiplied equals straight color.
        let c = px.demultiply();
        let [r, g, b] = flip([c.red(), c.green(), c.blue()]);
        *px = ColorU8::from_rgba(r, g, b, 255).premultiply();
    }
}

/// One color with its lightness flipped.
fn flip(rgb: [u8; 3]) -> [u8; 3] {
    let [r, g, b] = rgb.map(|v| v as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let s = s.min(1.0);
    // Vivid accents (cyan, yellow) look brighter than their lightness says:
    // take them a step darker so they read on the light background. Greys
    // (no saturation) are untouched, so edges blend smoothly.
    let l = (LIGHTEST - (LIGHTEST - DARKEST) * l - ACCENT_DEPTH * s).max(DARKEST);
    // Judged by chroma, not saturation: near white or black, saturation
    // calls the faintest tint vivid.
    if d < NEUTRAL {
        return panel_grey(l);
    }
    hsl_to_rgb(h, s, l)
}

/// Below this chroma (spread between channels) a color is a pure grey.
const NEUTRAL: f32 = 0.1;

/// A grey the RGB565 panel shows without a tint, at lightness `l`. Red and
/// blue have 32 levels and green 64, so most greys come out a little
/// magenta or green; this picks green's level to match, and lands each
/// channel exactly on a panel level so dithering leaves it flat.
fn panel_grey(l: f32) -> [u8; 3] {
    let r5 = (l * 31.0).round().clamp(0.0, 31.0) as u8;
    let g6 = (f32::from(r5) * 63.0 / 31.0).round() as u8;
    [r5 << 3, g6 << 2, r5 << 3]
}

/// `h` in sixths of a turn (0..6), `s` and `l` 0..1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lightness(rgb: [u8; 3]) -> f32 {
        let max = *rgb.iter().max().unwrap() as f32;
        let min = *rgb.iter().min().unwrap() as f32;
        (max + min) / 2.0 / 255.0
    }

    #[test]
    fn black_and_white_swap() {
        assert!(lightness(flip([0, 0, 0])) > 0.9);
        assert!(lightness(flip([255, 255, 255])) < 0.1);
        // Greys stay grey (on the panel: see greys_stay_neutral_on_the_panel).
        let [r, _, b] = flip([0x50, 0x50, 0x50]);
        assert_eq!(r, b);
    }

    #[test]
    fn greys_stay_neutral_on_the_panel() {
        for v in (0..=255).step_by(5) {
            let [r, g, b] = flip([v, v, v]);
            assert_eq!(r, b);
            // Exactly on panel levels: dithering can't change them.
            assert_eq!((r % 8, g % 4), (0, 0), "{v}");
            // Green's share matches red's within half a level.
            let (red, green) = (f32::from(r >> 3) / 31.0, f32::from(g >> 2) / 63.0);
            assert!(
                (red - green).abs() <= 0.5 / 63.0 + 1e-6,
                "{v}: {red} vs {green}"
            );
        }
    }

    #[test]
    fn accents_keep_their_hue_and_darken() {
        let cyan = [0x30, 0xe8, 0xf0];
        let deep = flip(cyan);
        // Still blue-green: red lowest, green and blue high.
        assert!(deep[0] < deep[1] && deep[0] < deep[2], "{deep:?}");
        assert!(lightness(deep) < lightness(cyan));
    }

    #[test]
    fn dark_leaves_the_frame_alone() {
        let mut pm = Pixmap::new(2, 1).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(10, 200, 30, 255));
        let before = pm.clone();
        apply(&mut pm, Shade::Dark);
        assert_eq!(pm, before);
        apply(&mut pm, Shade::Light);
        assert_ne!(pm, before);
    }
}
