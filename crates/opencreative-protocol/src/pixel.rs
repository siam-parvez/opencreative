//! Pixel format conversion.

/// Packs 8-bit RGB into RGB565.
pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}

/// Converts tightly packed RGBA8 pixels to RGB565 little-endian bytes.
/// Alpha is ignored; callers flatten transparency before converting.
pub fn rgba_to_rgb565_le(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 2);
    for [r, g, b, _] in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&rgb565(*r, *g, *b).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primaries() {
        assert_eq!(rgb565(255, 0, 0), 0xF800);
        assert_eq!(rgb565(0, 255, 0), 0x07E0);
        assert_eq!(rgb565(0, 0, 255), 0x001F);
        assert_eq!(rgb565(255, 255, 255), 0xFFFF);
    }

    #[test]
    fn little_endian_output() {
        assert_eq!(rgba_to_rgb565_le(&[255, 0, 0, 255]), [0x00, 0xF8]);
    }
}
