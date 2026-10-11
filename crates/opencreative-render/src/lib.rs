//! Produces frames for cooler screens.
//!
//! Everything renders into a `tiny_skia::Pixmap` at the *logical* size (the
//! panel size, swapped for 90/270 degree rotation), then [`rotate`] maps it to
//! the panel's native orientation. Pixmaps are always fully opaque, so their
//! premultiplied RGBA bytes can be sent to a device as-is.

pub mod icon;
pub mod media;
pub mod monitor;
pub mod panels;
mod shapes;
mod text;
pub mod video;
pub mod zen;

pub use tiny_skia::Pixmap;

/// How the cooler is mounted, i.e. how far to rotate content clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn from_degrees(deg: u16) -> Option<Self> {
        match deg {
            0 => Some(Self::R0),
            90 => Some(Self::R90),
            180 => Some(Self::R180),
            270 => Some(Self::R270),
            _ => None,
        }
    }

    /// Size to render at so the result fills a `panel_w` x `panel_h` panel.
    pub fn logical_size(self, panel_w: u32, panel_h: u32) -> (u32, u32) {
        match self {
            Self::R0 | Self::R180 => (panel_w, panel_h),
            Self::R90 | Self::R270 => (panel_h, panel_w),
        }
    }
}

/// Rotates a logical-size pixmap clockwise into panel orientation.
pub fn rotate(src: &Pixmap, rotation: Rotation) -> Pixmap {
    if rotation == Rotation::R0 {
        return src.clone();
    }
    let (sw, sh) = (src.width(), src.height());
    let (dw, dh) = match rotation {
        Rotation::R90 | Rotation::R270 => (sh, sw),
        _ => (sw, sh),
    };
    let mut dst = Pixmap::new(dw, dh).expect("non-zero size");
    let s = src.pixels();
    let d = dst.pixels_mut();
    for sy in 0..sh {
        for sx in 0..sw {
            let (dx, dy) = match rotation {
                Rotation::R90 => (sh - 1 - sy, sx),
                Rotation::R180 => (sw - 1 - sx, sh - 1 - sy),
                Rotation::R270 => (sy, sw - 1 - sx),
                Rotation::R0 => unreachable!(),
            };
            d[(dy * dw + dx) as usize] = s[(sy * sw + sx) as usize];
        }
    }
    dst
}

/// A single solid color.
pub fn solid(width: u32, height: u32, rgb: [u8; 3]) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    pm.fill(tiny_skia::Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255));
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x1 image: red, green.
    fn strip() -> Pixmap {
        let mut pm = Pixmap::new(2, 1).unwrap();
        pm.data_mut()
            .copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255]);
        pm
    }

    #[test]
    fn rotate_90_moves_left_pixel_to_top() {
        let out = rotate(&strip(), Rotation::R90);
        assert_eq!((out.width(), out.height()), (1, 2));
        assert_eq!(&out.data()[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn rotate_180_and_270() {
        let out = rotate(&strip(), Rotation::R180);
        assert_eq!(&out.data()[..4], &[0, 255, 0, 255]);
        let out = rotate(&strip(), Rotation::R270);
        assert_eq!(&out.data()[..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn logical_size_swaps_for_portrait() {
        assert_eq!(Rotation::R90.logical_size(320, 240), (240, 320));
        assert_eq!(Rotation::R180.logical_size(320, 240), (320, 240));
    }
}
