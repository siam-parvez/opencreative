//! Anti-aliased text drawing with a bundled font.

use std::sync::LazyLock;

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _, VariableFont as _, point};
use tiny_skia::Pixmap;

// Rajdhani, SIL Open Font License 1.1 (assets/fonts/OFL-Rajdhani.txt).
static BOLD: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(include_bytes!("../../../assets/fonts/Rajdhani-Bold.ttf"))
        .expect("bundled font is valid")
});
// Orbitron, SIL Open Font License 1.1 (assets/fonts/OFL-Orbitron.txt).
static DISPLAY: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    let mut font = FontRef::try_from_slice(include_bytes!("../../../assets/fonts/Orbitron.ttf"))
        .expect("bundled font is valid");
    // A variable font: pick a semibold weight for large numerals.
    font.set_variation(b"wght", 600.0);
    font
});
static SEMIBOLD: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(include_bytes!(
        "../../../assets/fonts/Rajdhani-SemiBold.ttf"
    ))
    .expect("bundled font is valid")
});

#[derive(Clone, Copy)]
pub enum Weight {
    Bold,
    SemiBold,
    /// Squared geometric numerals for large readings.
    Display,
}

#[derive(Clone, Copy)]
pub enum Align {
    Left,
    Center,
    Right,
}

fn face(weight: Weight) -> &'static FontRef<'static> {
    match weight {
        Weight::Bold => &BOLD,
        Weight::SemiBold => &SEMIBOLD,
        Weight::Display => &DISPLAY,
    }
}

/// Advance width of `text` in pixels.
pub fn measure(text: &str, size: f32, weight: Weight) -> f32 {
    let font = face(weight).as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            width += font.kern(p, id);
        }
        width += font.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Draws `text` anchored horizontally at `x` and vertically centered on `cy`.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pm: &mut Pixmap,
    text: &str,
    x: f32,
    cy: f32,
    size: f32,
    weight: Weight,
    align: Align,
    rgb: [u8; 3],
) {
    let font_ref = face(weight);
    let font = font_ref.as_scaled(PxScale::from(size));
    let width = measure(text, size, weight);
    let left = match align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
        Align::Right => x - width,
    };
    let baseline = cy + (font.ascent() + font.descent()) / 2.0;

    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let data = pm.data_mut();
    let mut caret = left;
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            caret += font.kern(p, id);
        }
        let glyph = id.with_scale_and_position(size, point(caret, baseline));
        caret += font.h_advance(id);
        prev = Some(id);

        let Some(outline) = font_ref.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i32 + gx as i32;
            let py = bounds.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= pw || py >= ph {
                return;
            }
            let i = (py * pw + px) as usize * 4;
            let a = coverage.clamp(0.0, 1.0);
            for (ch, &src) in rgb.iter().enumerate() {
                let dst = f32::from(data[i + ch]);
                data[i + ch] = (f32::from(src) * a + dst * (1.0 - a)).round() as u8;
            }
        });
    }
}
