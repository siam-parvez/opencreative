//! Renders the Panels screen with sample values:
//! `cargo run --example panels -- out.png [lines|hud] [portrait]`.
use opencreative_render::panels::{Layout, Skin, render};
use opencreative_sensors::{Gpu, Metric, Snapshot};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("output path");
    let skin = match args.next().as_deref() {
        Some("hud") => Skin::Hud,
        _ => Skin::Lines,
    };
    let (w, h) = match args.next().as_deref() {
        Some("portrait") => (240, 320),
        _ => (320, 240),
    };
    // A minute of made-up history so charts have something to show.
    let history: Vec<Snapshot> = (0..60)
        .map(|i| {
            let t = i as f32;
            Snapshot {
                cpu_temp_c: Some(50.0 + 6.0 * (t / 7.0).sin()),
                gpu: Gpu {
                    temp_c: Some(28.0),
                    load_pct: Some(70.0 + 20.0 * (t / 4.0).sin().abs()),
                    power_w: Some(255.0),
                    ..Gpu::default()
                },
                net_rx_bps: 120e6 + 60e6 * (t / 3.0).sin() as f64,
                net_tx_bps: 25e6 + 10e6 * (t / 5.0).cos() as f64,
                ..Snapshot::default()
            }
        })
        .collect();
    let mut snap = history.last().cloned().unwrap_or_default();
    snap.cpu_temp_c = Some(55.0);
    snap.gpu.load_pct = Some(89.0);
    let layout = match skin {
        Skin::Lines => Layout {
            primary: Metric::CpuTemp,
            secondary: [Metric::GpuTemp, Metric::GpuPower],
        },
        Skin::Hud => Layout {
            primary: Metric::CpuTemp,
            secondary: [Metric::GpuLoad, Metric::GpuTemp],
        },
    };
    render(&snap, &history, &layout, skin, w, h)
        .save_png(path)
        .unwrap();
}
