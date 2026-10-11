//! Renders the Panels screen with sample values: `cargo run --example panels -- out.png`.
use opencreative_render::panels::{Layout, render};
use opencreative_sensors::{Metric, Snapshot};

fn main() {
    let path = std::env::args().nth(1).expect("output path");
    let snap = Snapshot {
        cpu_temp_c: Some(72.0),
        cpu_mhz: 5500,
        cpu_power_w: Some(66.1),
        ..Snapshot::default()
    };
    let layout = Layout {
        primary: Metric::CpuTemp,
        secondary: [Metric::CpuFreq, Metric::CpuPower],
    };
    let (w, h) = match std::env::args().nth(2).as_deref() {
        Some("portrait") => (240, 320),
        _ => (320, 240),
    };
    render(&snap, &layout, w, h).save_png(path).unwrap();
}
