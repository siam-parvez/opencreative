//! Dashboard: live readings from this computer, laid out like the vendor
//! app's (CPU and GPU gauges, memory, storage, network), plus Claude usage
//! when that integration is on, and a history chart.

use eframe::egui::{self, Color32, Sense, Stroke, pos2, vec2};
use opencreative_sensors::{ClaudeWindow, Snapshot};

use crate::App;
use crate::icons::{self, Icon};
use crate::pages::{card_title, header};
use crate::theme::{self, Palette};
use crate::widgets::{self, bytes, rate, stat};

/// More disks than this get summarized rather than listed.
const MAX_DISKS: usize = 3;
const GAUGE: f32 = 156.0;
const SPARK_HEIGHT: f32 = 46.0;
const CLAUDE: Color32 = Color32::from_rgb(0xd9, 0x77, 0x57);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    header(ui, "Dashboard", Some("Live readings from this computer"));
    let snap = app.history.latest().cloned().unwrap_or_default();
    let samples = app.history.samples();
    let gl_name = app.gpu_name.clone();

    card_row(ui, "processors", 2, |i, ui| match i {
        0 => cpu(ui, &snap, &samples),
        _ => gpu(ui, &snap, &samples, gl_name.as_deref()),
    });
    ui.add_space(12.0);
    card_row(ui, "system", 3, |i, ui| match i {
        0 => memory(ui, &snap),
        1 => storage(ui, &snap),
        _ => network(ui, &snap, &samples),
    });
    ui.add_space(12.0);
    if app.prefs.claude_usage {
        card_row(ui, "activity", 2, |i, ui| match i {
            0 => claude(ui, &snap),
            _ => history(app, ui),
        });
    } else {
        card_row(ui, "activity", 1, |_, ui| history(app, ui));
    }
    // Room so the status pill never covers the last card.
    ui.add_space(56.0);
}

/// Cards side by side, all as tall as the tallest one. Heights are measured
/// each frame and applied on the next, which settles immediately.
fn card_row(ui: &mut egui::Ui, id: &str, n: usize, mut add: impl FnMut(usize, &mut egui::Ui)) {
    let key = egui::Id::new(("card-row-height", id));
    let target: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
    let mut tallest = 0.0_f32;
    ui.columns(n, |cols| {
        for (i, col) in cols.iter_mut().enumerate() {
            theme::card(col, |ui| {
                let top = ui.cursor().min.y;
                ui.set_min_height(target);
                add(i, ui);
                tallest = tallest.max(ui.cursor().min.y - top);
            });
        }
    });
    if (tallest - target).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, tallest));
        ui.ctx().request_repaint();
    }
}

fn cpu(ui: &mut egui::Ui, s: &Snapshot, samples: &[Snapshot]) {
    let name = (!s.cpu_name.is_empty()).then_some(s.cpu_name.as_str());
    card_title(ui, Icon::Cpu, "CPU", name);
    ui.horizontal(|ui| {
        widgets::gauge(
            ui,
            GAUGE,
            s.cpu_load_pct / 100.0,
            &number(s.cpu_load_pct),
            "%",
            "Load",
            ("0%", "100%"),
        );
        ui.add_space(12.0);
        stat_grid(
            ui,
            "cpu-stats",
            &[
                (mhz(s.cpu_mhz as f32), "Frequency"),
                (celsius(s.cpu_temp_c), "Temperature"),
                (
                    s.cpu_power_w
                        .map_or("—".to_owned(), |w| format!("{w:.1} W")),
                    "Power",
                ),
            ],
        );
    });
    let p = Palette::of(ui.ctx());
    sparkline(
        ui,
        &[
            (series(samples, |s| Some(s.cpu_load_pct / 100.0)), p.accent),
            (series(samples, |s| s.cpu_temp_c.map(|t| t / 100.0)), p.warn),
        ],
        "Load and temperature, last two minutes",
    );
}

fn gpu(ui: &mut egui::Ui, s: &Snapshot, samples: &[Snapshot], gl_name: Option<&str>) {
    let g = &s.gpu;
    card_title(ui, Icon::Gpu, "GPU", g.name.as_deref().or(gl_name));
    ui.horizontal(|ui| {
        let (fraction, value, unit) = match g.load_pct {
            Some(load) => (load / 100.0, number(load), "%"),
            None => (0.0, "—".to_owned(), ""),
        };
        widgets::gauge(ui, GAUGE, fraction, &value, unit, "Load", ("0%", "100%"));
        ui.add_space(12.0);
        let vram = match (g.vram_used_bytes, g.vram_total_bytes) {
            (Some(used), Some(total)) => format!(
                "{} / {}",
                bytes(used as f64).trim_end_matches(" GB"),
                bytes(total as f64)
            ),
            _ => "—".to_owned(),
        };
        stat_grid(
            ui,
            "gpu-stats",
            &[
                (
                    g.mhz
                        .filter(|m| *m > 0)
                        .map_or("—".to_owned(), |m| mhz(m as f32)),
                    "Frequency",
                ),
                (celsius(g.temp_c), "Temperature"),
                (vram, "Video memory"),
            ],
        );
    });
    let p = Palette::of(ui.ctx());
    if g.load_pct.is_some() || g.temp_c.is_some() {
        sparkline(
            ui,
            &[
                (
                    series(samples, |s| s.gpu.load_pct.map(|l| l / 100.0)),
                    p.accent,
                ),
                (series(samples, |s| s.gpu.temp_c.map(|t| t / 100.0)), p.warn),
            ],
            "Load and temperature, last two minutes",
        );
    } else {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("This GPU's driver reports frequency only.")
                .small()
                .color(p.faint),
        );
    }
}

fn memory(ui: &mut egui::Ui, s: &Snapshot) {
    let p = Palette::of(ui.ctx());
    card_title(ui, Icon::Memory, "Memory", None);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{:.0}", s.mem_used_pct))
                .font(theme::medium(34.0))
                .color(p.text),
        );
        ui.label(egui::RichText::new("% used").color(p.muted));
    });
    widgets::usage_bar(ui, s.mem_used_pct / 100.0, 22.0);
    ui.add_space(8.0);
    let free = s.mem_total_bytes.saturating_sub(s.mem_used_bytes);
    stat_grid(
        ui,
        "memory-stats",
        &[
            (bytes(s.mem_used_bytes as f64), "In use"),
            (bytes(free as f64), "Available"),
            (bytes(s.mem_total_bytes as f64), "Total"),
        ],
    );
}

fn storage(ui: &mut egui::Ui, s: &Snapshot) {
    let p = Palette::of(ui.ctx());
    card_title(ui, Icon::Storage, "Storage", None);
    ui.horizontal(|ui| {
        rate_chip(ui, Icon::Up, &rate(s.disk_read_bps), "read");
        rate_chip(ui, Icon::Down, &rate(s.disk_write_bps), "write");
    });
    ui.add_space(6.0);
    for disk in s.disks.iter().take(MAX_DISKS) {
        let fraction = if disk.total_bytes == 0 {
            0.0
        } else {
            disk.used_bytes as f32 / disk.total_bytes as f32
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&disk.name).color(p.text))
                .on_hover_text(&disk.mount_point);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} / {}",
                        bytes(disk.used_bytes as f64),
                        bytes(disk.total_bytes as f64)
                    ))
                    .small()
                    .color(p.muted),
                );
            });
        });
        widgets::usage_bar(ui, fraction, 10.0);
        ui.add_space(4.0);
    }
    let hidden = s.disks.len().saturating_sub(MAX_DISKS);
    if hidden > 0 {
        ui.label(
            egui::RichText::new(format!("and {hidden} more"))
                .small()
                .color(p.faint),
        );
    }
}

fn network(ui: &mut egui::Ui, s: &Snapshot, samples: &[Snapshot]) {
    let p = Palette::of(ui.ctx());
    card_title(ui, Icon::Network, "Network", None);
    ui.horizontal(|ui| {
        rate_chip(ui, Icon::Up, &rate(s.net_tx_bps), "up");
        rate_chip(ui, Icon::Down, &rate(s.net_rx_bps), "down");
    });
    let peak = samples
        .iter()
        .flat_map(|s| [s.net_rx_bps, s.net_tx_bps])
        .fold(1.0_f64, f64::max);
    sparkline(
        ui,
        &[
            (
                series(samples, |s| Some((s.net_rx_bps / peak) as f32)),
                p.accent,
            ),
            (
                series(samples, |s| Some((s.net_tx_bps / peak) as f32)),
                p.muted,
            ),
        ],
        &format!("Download and upload, peak {}", rate(peak)),
    );
}

fn claude(ui: &mut egui::Ui, s: &Snapshot) {
    let p = Palette::of(ui.ctx());
    card_title(ui, Icon::Dashboard, "Claude usage", Some("via OpenCUT"));
    let Some(usage) = s.claude else {
        ui.label(
            egui::RichText::new(
                "No usage yet. Start OpenCUT's ocutd service to see your Claude plan limits here.",
            )
            .color(p.muted),
        );
        return;
    };
    for (label, window) in [("5-hour session", usage.session), ("Weekly", usage.weekly)] {
        claude_window(ui, label, window);
        ui.add_space(10.0);
    }
}

fn claude_window(ui: &mut egui::Ui, label: &str, window: Option<ClaudeWindow>) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let pct = window.map_or("—".to_owned(), |w| format!("{:.0}%", w.pct));
            ui.label(
                egui::RichText::new(pct)
                    .font(theme::semibold(17.0))
                    .color(CLAUDE),
            );
        });
    });
    widgets::usage_bar_colored(ui, window.map_or(0.0, |w| w.pct / 100.0), 14.0, CLAUDE);
    let reset = window
        .and_then(|w| w.resets_in_secs)
        .map_or("No active window".to_owned(), |s| {
            format!("Resets in {}", countdown(s))
        });
    ui.label(egui::RichText::new(reset).small().color(p.muted));
}

fn history(app: &mut App, ui: &mut egui::Ui) {
    card_title(ui, Icon::Dashboard, "History", Some("last two minutes"));
    app.history.chart_with_legend(ui);
}

/// Accent values over muted labels, two per row.
fn stat_grid(ui: &mut egui::Ui, id: &str, stats: &[(String, &str)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(vec2(18.0, 10.0))
        .show(ui, |ui| {
            for (i, (value, label)) in stats.iter().enumerate() {
                stat(ui, value, label);
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
}

/// An arrow icon, a bold rate and a muted word.
fn rate_chip(ui: &mut egui::Ui, icon: Icon, value: &str, what: &str) {
    let p = Palette::of(ui.ctx());
    icons::show(ui, icon, 16.0, p.faint);
    ui.label(
        egui::RichText::new(value)
            .font(theme::medium(15.0))
            .color(p.text),
    );
    ui.label(egui::RichText::new(what).color(p.muted));
    ui.add_space(8.0);
}

fn series(samples: &[Snapshot], f: impl Fn(&Snapshot) -> Option<f32>) -> Vec<Option<f32>> {
    samples.iter().map(f).collect()
}

/// A small line chart of 0..1 values, newest at the right, with a caption.
fn sparkline(ui: &mut egui::Ui, lines: &[(Vec<Option<f32>>, Color32)], caption: &str) {
    const SLOTS: usize = 120;
    let p = Palette::of(ui.ctx());
    ui.add_space(8.0);
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), SPARK_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, p.control);
    let inner = rect.shrink2(vec2(6.0, 6.0));
    let dx = inner.width() / (SLOTS - 1) as f32;
    for (values, color) in lines {
        let recent = &values[values.len().saturating_sub(SLOTS)..];
        let offset = SLOTS - recent.len();
        let mut run = Vec::new();
        let flush = |run: &mut Vec<egui::Pos2>| {
            if run.len() > 1 {
                painter.line(std::mem::take(run), Stroke::new(1.5, *color));
            }
            run.clear();
        };
        for (i, v) in recent.iter().enumerate() {
            match v {
                Some(v) => run.push(pos2(
                    inner.left() + (offset + i) as f32 * dx,
                    inner.bottom() - v.clamp(0.0, 1.0) * inner.height(),
                )),
                None => flush(&mut run),
            }
        }
        flush(&mut run);
    }
    ui.label(egui::RichText::new(caption).small().color(p.faint));
}

fn countdown(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

fn number(v: f32) -> String {
    if v >= 10.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn mhz(mhz: f32) -> String {
    if mhz <= 0.0 {
        "—".to_owned()
    } else {
        format!("{mhz:.0} MHz")
    }
}

fn celsius(c: Option<f32>) -> String {
    c.map_or("—".to_owned(), |c| format!("{c:.0} °C"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdowns_are_short() {
        assert_eq!(countdown(15794), "4h 23m");
        assert_eq!(countdown(513794), "5d 22h");
        assert_eq!(countdown(300), "5m");
    }
}
