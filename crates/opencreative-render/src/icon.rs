//! The OpenCreative app icon, drawn in code so every size comes from one
//! source: a dark rounded tile with a gauge ring, like the monitor screen.

use tiny_skia::{Paint, Pixmap, Transform};

use crate::monitor::{arc, rgb};
use crate::shapes::rounded_rect;
use crate::text::{self, Align, Weight};

const TILE: [u8; 3] = [0x10, 0x14, 0x20];
const TRACK: [u8; 3] = [0x24, 0x2c, 0x3d];
const ACCENT: [u8; 3] = [0x22, 0xd3, 0xee];
const TEXT: [u8; 3] = [0xf1, 0xf5, 0xf9];

/// Renders the icon at `size` x `size`, transparent outside the tile.
pub fn icon(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).expect("non-zero size");
    let s = size as f32;

    let inset = s * 0.04;
    let side = s - 2.0 * inset;
    if let Some(tile) = rounded_rect(inset, inset, side, side, s * 0.2) {
        let mut paint = Paint::default();
        paint.set_color(rgb(TILE));
        paint.anti_alias = true;
        pm.fill_path(
            &tile,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let (c, r, width) = (s / 2.0, s * 0.31, s * 0.085);
    arc(&mut pm, c, c, r, 1.0, width, TRACK);
    arc(&mut pm, c, c, r, 0.68, width, ACCENT);
    text::draw(
        &mut pm,
        "OC",
        c,
        c,
        s * 0.3,
        Weight::Bold,
        Align::Center,
        TEXT,
    );
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_is_opaque_and_corners_transparent() {
        let pm = icon(64);
        let alpha = |x: u32, y: u32| pm.pixel(x, y).unwrap().alpha();
        assert_eq!(alpha(32, 10), 255);
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(63, 63), 0);
    }
}
