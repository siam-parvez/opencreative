//! Renders the Panels screen with sample values:
//! `cargo run --example panels -- out.png [lines|hud|gauges] [landscape|portrait] [claude]`.
//! Set `OPENCREATIVE_SHADE=light` for the light version.
use opencreative_render::panels::{Layout, Skin, render};
use opencreative_sensors::{ClaudeUsage, ClaudeWindow, Gpu, Metric, Snapshot};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("output path");
    let which = args.next();
    let skin = match which.as_deref() {
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
    let claude = args.next().as_deref() == Some("claude");
    snap.claude = Some(ClaudeUsage {
        session: Some(ClaudeWindow {
            pct: 42.0,
            resets_in_secs: Some(9_000),
        }),
        weekly: Some(ClaudeWindow {
            pct: 58.0,
            resets_in_secs: Some(330_000),
        }),
    });
    let layout = match skin {
        _ if claude => Layout {
            primary: Metric::ClaudeSession,
            secondary: [Metric::ClaudeWeekly, Metric::CpuTemp],
        },
        Skin::Lines => Layout {
            primary: Metric::CpuTemp,
            secondary: [Metric::GpuTemp, Metric::GpuPower],
        },
        Skin::Hud => Layout {
            primary: Metric::CpuTemp,
            secondary: [Metric::GpuLoad, Metric::GpuTemp],
        },
    };
    snap.cpu_load_pct = 34.0;
    snap.mem_used_pct = 41.0;
    let frame = if which.as_deref() == Some("gauges") {
        opencreative_render::monitor::render(&snap, "18:28", w, h)
    } else {
        render(&snap, &history, &layout, skin, w, h)
    };
    let mut frame = frame;
    if std::env::var("OPENCREATIVE_SHADE").as_deref() == Ok("light") {
        opencreative_render::shade::apply(&mut frame, opencreative_render::Shade::Light);
    }
    frame.save_png(path).unwrap();
}
