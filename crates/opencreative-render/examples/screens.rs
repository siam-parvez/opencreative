//! Renders the music bars and the Zen screen with sample values, for the
//! README: `cargo run --example screens -- <dir>` writes `music.png`,
//! `music-mirror.png` and `zen.png` there, 320x240.
use opencreative_render::music::{self, BarStyle, Look};
use opencreative_sensors::Snapshot;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));

    // A made-up spectrum: strong bass, a bump in the mids, falling treble.
    let levels: Vec<f32> = (0..24)
        .map(|i| {
            let x = i as f32 / 23.0;
            let bass = (1.0 - x * 2.2).max(0.0) * 0.75;
            let mids = (-(x - 0.45).powi(2) * 30.0).exp() * 0.6;
            let wobble = 0.08 * (i as f32 * 1.7).sin().abs();
            (0.12 + bass + mids + wobble).min(0.95)
        })
        .collect();
    let peaks: Vec<f32> = levels.iter().map(|l| (l + 0.08).min(1.0)).collect();

    let rainbow = Look {
        style: BarStyle::Bars,
        color: [0x22, 0xd3, 0xee],
        rainbow: true,
    };
    music::render(&levels, &peaks, &rainbow, None, 320, 240)
        .save_png(dir.join("music.png"))
        .unwrap();

    let mirror = Look {
        style: BarStyle::Mirror,
        color: [0x8b, 0x5c, 0xf6],
        rainbow: false,
    };
    music::render(&levels, &peaks, &mirror, None, 320, 240)
        .save_png(dir.join("music-mirror.png"))
        .unwrap();

    let snap = Snapshot {
        cpu_temp_c: Some(41.0),
        cpu_load_pct: 6.0,
        ..Snapshot::default()
    };
    opencreative_render::zen::render(&snap, "18:28", 320, 240)
        .save_png(dir.join("zen.png"))
        .unwrap();
}
