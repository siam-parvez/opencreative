//! Pixel format conversion.

/// Packs 8-bit RGB into RGB565.
pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}

/// 4x4 Bayer threshold matrix, values 0..16.
const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Converts tightly packed RGBA8 pixels (`width` pixels per row) to RGB565
/// little-endian bytes. Alpha is ignored; callers flatten transparency first.
///
/// Uses ordered dithering: with only 32 levels for red and blue, smooth
/// gradients otherwise show as visible horizontal bands. Each threshold spans
/// one quantization step (8 levels for 5-bit channels, 4 for 6-bit green), so
/// pure black and white are unchanged.
pub fn rgba_to_rgb565_le(rgba: &[u8], width: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 2);
    for (i, [r, g, b, _]) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let t = BAYER4[(i / width) % 4][i % width % 4];
        let px = rgb565(
            r.saturating_add(t / 2),
            g.saturating_add(t / 4),
            b.saturating_add(t / 2),
        );
        out.extend_from_slice(&px.to_le_bytes());
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
        assert_eq!(rgba_to_rgb565_le(&[255, 0, 0, 255], 1), [0x00, 0xF8]);
    }

    fn block(rgb: [u8; 3]) -> Vec<u16> {
        let rgba: Vec<u8> = (0..16)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect();
        rgba_to_rgb565_le(&rgba, 4)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect()
    }

    #[test]
    fn black_and_white_are_exact() {
        assert!(block([0, 0, 0]).iter().all(|&p| p == 0x0000));
        assert!(block([255, 255, 255]).iter().all(|&p| p == 0xFFFF));
    }

    #[test]
    fn in_between_levels_average_out() {
        // Red 4 sits halfway between 5-bit levels 0 and 1 (0 and 8 in 8-bit),
        // so half of a 4x4 block should round up.
        let ups = block([4, 0, 0]).iter().filter(|&&p| p >> 11 == 1).count();
        assert_eq!(ups, 8);
    }
}
