//! Dashboard: live readings from this computer, laid out like the vendor
//! app's (CPU and GPU gauges, memory, storage, network) plus a history chart.

use eframe::egui;
use opencreative_sensors::Snapshot;

use crate::App;
use crate::icons::{self, Icon};
use crate::pages::{card_title, header};
use crate::theme::{self, Palette};
use crate::widgets::{self, bytes, rate, stat};

const ROW_ONE_HEIGHT: f32 = 230.0;
const ROW_TWO_HEIGHT: f32 = 236.0;
/// More disks than this get summarized rather than listed.
const MAX_DISKS: usize = 3;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    header(ui, "Dashboard", Some("Live readings from this computer"));
    let snap = app.history.latest().cloned().unwrap_or_default();

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], |ui| {
            ui.set_min_height(ROW_ONE_HEIGHT);
            cpu(ui, &snap);
        });
        theme::card(&mut cols[1], |ui| {
            ui.set_min_height(ROW_ONE_HEIGHT);
            gpu(ui, &snap, app.gpu_name.as_deref());
        });
    });
    ui.add_space(12.0);
    ui.columns(3, |cols| {
        theme::card(&mut cols[0], |ui| {
            ui.set_min_height(ROW_TWO_HEIGHT);
            memory(ui, &snap);
        });
        theme::card(&mut cols[1], |ui| {
            ui.set_min_height(ROW_TWO_HEIGHT);
            storage(ui, &snap);
        });
        theme::card(&mut cols[2], |ui| {
            ui.set_min_height(ROW_TWO_HEIGHT);
            network(ui, &snap);
        });
    });
    ui.add_space(12.0);
    theme::card(ui, |ui| {
        card_title(ui, Icon::Dashboard, "History", Some("last two minutes"));
        app.history.chart_with_legend(ui);
    });
    // Room so the status pill never covers the last card.
    ui.add_space(56.0);
}

fn cpu(ui: &mut egui::Ui, s: &Snapshot) {
    let name = (!s.cpu_name.is_empty()).then_some(s.cpu_name.as_str());
    card_title(ui, Icon::Cpu, "CPU", name);
    ui.horizontal(|ui| {
        widgets::gauge(
            ui,
            180.0,
            s.cpu_load_pct / 100.0,
            &one_decimal(s.cpu_load_pct),
            "%",
            "Load",
            ("0%", "100%"),
        );
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            stat(ui, &mhz(s.cpu_mhz as f32), "Frequency");
            ui.add_space(6.0);
            stat(ui, &celsius(s.cpu_temp_c), "Temperature");
            ui.add_space(6.0);
            let power = s
                .cpu_power_w
                .map_or("—".to_owned(), |w| format!("{w:.1} W"));
            ui.scope(|ui| stat(ui, &power, "Power"))
                .response
                .on_hover_text(if s.cpu_power_w.is_some() {
                    "Package power from RAPL"
                } else {
                    "Needs read access to RAPL energy counters (root-only by default on Linux)"
                });
        });
    });
}

fn gpu(ui: &mut egui::Ui, s: &Snapshot, gl_name: Option<&str>) {
    let g = &s.gpu;
    let name = g.name.as_deref().or(gl_name);
    card_title(ui, Icon::Gpu, "GPU", name);
    ui.horizontal(|ui| {
        let (fraction, value, unit) = match g.load_pct {
            Some(load) => (load / 100.0, one_decimal(load), "%"),
            None => (0.0, "—".to_owned(), ""),
        };
        widgets::gauge(ui, 180.0, fraction, &value, unit, "Load", ("0%", "100%"));
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            let freq = g
                .mhz
                .filter(|m| *m > 0)
                .map_or("—".to_owned(), |m| mhz(m as f32));
            stat(ui, &freq, "Frequency");
            ui.add_space(6.0);
            stat(ui, &celsius(g.temp_c), "Temperature");
            ui.add_space(6.0);
            let vram = match (g.vram_used_bytes, g.vram_total_bytes) {
                (Some(used), Some(total)) => format!(
                    "{} / {}",
                    bytes(used as f64).trim_end_matches(" GB"),
                    bytes(total as f64)
                ),
                _ => "—".to_owned(),
            };
            stat(ui, &vram, "Video memory");
        });
    });
    if g.load_pct.is_none() && g.temp_c.is_none() {
        let p = Palette::of(ui.ctx());
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("This GPU's driver reports only some readings.")
                .small()
                .color(p.faint),
        );
    }
}

fn memory(ui: &mut egui::Ui, s: &Snapshot) {
    let p = Palette::of(ui.ctx());
    card_title(ui, Icon::Memory, "Memory", None);
    widgets::usage_bar(ui, s.mem_used_pct / 100.0, 46.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} / {}",
                bytes(s.mem_used_bytes as f64),
                bytes(s.mem_total_bytes as f64)
            ))
            .small()
            .color(p.muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{:.0}%", s.mem_used_pct))
                    .small()
                    .color(p.muted),
            );
        });
    });
    ui.add_space(10.0);
    let free = s.mem_total_bytes.saturating_sub(s.mem_used_bytes);
    stat(ui, &bytes(s.mem_used_bytes as f64), "In use");
    ui.add_space(6.0);
    stat(ui, &bytes(free as f64), "Available");
}

fn storage(ui: &mut egui::Ui, s: &Snapshot) {
    let p = Palette::of(ui.ctx());
    card_title(ui, Icon::Storage, "Storage", None);
    rate_row(ui, Icon::Up, &rate(s.disk_read_bps), "read");
    rate_row(ui, Icon::Down, &rate(s.disk_write_bps), "write");
    ui.add_space(6.0);
    for disk in s.disks.iter().take(MAX_DISKS) {
        let fraction = if disk.total_bytes == 0 {
            0.0
        } else {
            disk.used_bytes as f32 / disk.total_bytes as f32
        };
        ui.label(egui::RichText::new(&disk.name).color(p.text))
            .on_hover_text(&disk.mount_point);
        widgets::usage_bar(ui, fraction, 14.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} / {}",
                    bytes(disk.used_bytes as f64),
                    bytes(disk.total_bytes as f64)
                ))
                .small()
                .color(p.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{:.0}%", fraction * 100.0))
                        .small()
                        .color(p.muted),
                );
            });
        });
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

fn network(ui: &mut egui::Ui, s: &Snapshot) {
    card_title(ui, Icon::Network, "Network", None);
    rate_row(ui, Icon::Up, &rate(s.net_tx_bps), "upload");
    rate_row(ui, Icon::Down, &rate(s.net_rx_bps), "download");
}

/// An arrow icon, a bold rate and a muted word, e.g. "↑ 8.3 KB/s upload".
fn rate_row(ui: &mut egui::Ui, icon: Icon, value: &str, what: &str) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        icons::show(ui, icon, 16.0, p.faint);
        ui.label(
            egui::RichText::new(value)
                .font(theme::medium(15.0))
                .color(p.text),
        );
        ui.label(egui::RichText::new(what).color(p.muted));
    });
}

fn one_decimal(v: f32) -> String {
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
