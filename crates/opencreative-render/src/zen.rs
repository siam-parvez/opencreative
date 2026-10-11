//! Zen mode: a calm, nearly empty screen with a dim clock and one quiet
//! reading, for when the full dashboard would be a distraction.

use opencreative_sensors::Snapshot;
use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::monitor::rgb;
use crate::text::{self, Align, Weight};

const CLOCK: [u8; 3] = [0x88, 0x8c, 0x90];
const QUIET: [u8; 3] = [0x50, 0x54, 0x58];
const RULE: [u8; 3] = [0x28, 0x2c, 0x30];

pub fn render(snap: &Snapshot, clock: &str, width: u32, height: u32) -> Pixmap {
    let mut pm = Pixmap::new(width, height).expect("non-zero size");
    pm.fill(rgb([0, 0, 0]));
    let (w, h) = (width as f32, height as f32);
    let unit = w.min(h);

    text::draw(
        &mut pm,
        clock,
        w / 2.0,
        h * 0.43,
        unit * 0.3,
        Weight::Display,
        Align::Center,
        CLOCK,
    );

    let rule_w = unit * 0.25;
    if let Some(rect) = Rect::from_xywh(
        ((w - rule_w) / 2.0).round(),
        (h * 0.62).round(),
        rule_w.round(),
        1.0,
    ) {
        let mut paint = Paint::default();
        paint.set_color(rgb(RULE));
        pm.fill_rect(rect, &paint, Transform::identity(), None);
    }

    let quiet = match snap.cpu_temp_c {
        Some(t) => format!("CPU {t:.0}°C"),
        None => format!("CPU {:.0}%", snap.cpu_load_pct),
    };
    text::draw(
        &mut pm,
        &quiet,
        w / 2.0,
        h * 0.72,
        unit * 0.065,
        Weight::Display,
        Align::Center,
        QUIET,
    );
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mostly_black_and_opaque() {
        let pm = render(&Snapshot::default(), "18:24", 320, 240);
        let pixels = pm.data().as_chunks::<4>().0;
        assert!(pixels.iter().all(|px| px[3] == 255));
        let black = pixels.iter().filter(|px| px[..3] == [0, 0, 0]).count();
        assert!(black * 10 > pixels.len() * 8, "at least 80% black");
    }
}
