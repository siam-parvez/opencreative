//! Dashboard: live readings from this computer, laid out like the vendor
//! app's (CPU and GPU gauges, memory, storage, network), plus Claude usage
//! when that integration is on, and a history chart.

use eframe::egui::{self, Color32, Sense, Stroke, pos2, vec2};
use opencreative_sensors::{ClaudeStatus, ClaudeWindow, Snapshot, countdown};

use crate::App;
use crate::icons::{self, CLAUDE, Icon};
use crate::pages::{card_title, header};
use crate::theme::{self, Palette};
use crate::widgets::{self, bytes, rate, stat};

/// More disks than this get summarized rather than listed.
const MAX_DISKS: usize = 3;
const GAUGE: f32 = 156.0;
const SPARK_HEIGHT: f32 = 52.0;
/// The GPU's color, as on the cooler screen.
const GPU: Color32 = Color32::from_rgb(0xf5, 0xb8, 0x41);

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

/// Cards side by side, all as tall as the tallest one, wrapping onto more
/// rows when a card would be narrower than `min_width`. Heights are measured
/// each frame and applied on the next; a frame whose heights were off is
/// redrawn before it's shown, so cards never visibly jump.
fn card_row(ui: &mut egui::Ui, id: &str, n: usize, mut add: impl FnMut(usize, &mut egui::Ui)) {
    let min_width = if n == 2 { 420.0 } else { 260.0 };
    let gap = ui.spacing().item_spacing.x;
    let fit = ((ui.available_width() + gap) / (min_width + gap)).floor() as usize;
    let per_row = fit.clamp(1, n);
    let mut first = 0;
    while first < n {
        let count = per_row.min(n - first);
        let key = egui::Id::new(("card-row-height", id, first));
        let target: f32 = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0.0);
        let mut tallest = 0.0_f32;
        ui.columns(per_row, |cols| {
            for (i, col) in cols.iter_mut().take(count).enumerate() {
                theme::card(col, |ui| {
                    let top = ui.cursor().min.y;
                    ui.set_min_height(target);
                    add(first + i, ui);
                    tallest = tallest.max(ui.cursor().min.y - top);
                });
            }
        });
        if (tallest - target).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(key, tallest));
            ui.ctx().request_discard("card heights changed");
        }
        first += count;
        if first < n {
            ui.add_space(gap);
        }
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
            "CPU load",
            Palette::of(ui.ctx()).accent,
        );
        ui.add_space(12.0);
        stat_grid(
            ui,
            "cpu-stats",
            &[
                (ghz(s.cpu_mhz as f32), "Frequency"),
                (celsius(s.cpu_temp_c), "Temperature"),
                (watts(s.cpu_power_w), "Power"),
                (pc_watts(s), "PC power (est.)"),
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
        widgets::gauge(ui, GAUGE, fraction, &value, unit, "GPU load", GPU);
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
                        .map_or("—".to_owned(), |m| ghz(m as f32)),
                    "Frequency",
                ),
                (celsius(g.temp_c), "Temperature"),
                (watts(g.power_w), "Power"),
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
                .font(theme::numeric(30.0))
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
    ui.horizontal_wrapped(|ui| {
        rate_chip(ui, Icon::Up, &rate(s.disk_read_bps), "read");
        ui.add_space(8.0);
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
    ui.horizontal_wrapped(|ui| {
        rate_chip(ui, Icon::Up, &rate(s.net_tx_bps), "up");
        ui.add_space(8.0);
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
    card_title(ui, Icon::Claude, "Claude", Some("plan usage"));
    let Some(usage) = s.claude else {
        let why = match opencreative_sensors::claude_status() {
            ClaudeStatus::NotSignedIn => {
                "Sign in to Claude Code on this computer to see your limits."
            }
            ClaudeStatus::Expired => {
                "Your Claude Code login expired. Run `claude` once to renew it."
            }
            ClaudeStatus::Unreachable => {
                "Can't reach Anthropic right now. Retrying in a few minutes."
            }
            ClaudeStatus::Off | ClaudeStatus::Ok => "Checking your limits…",
        };
        ui.label(egui::RichText::new(why).color(p.muted));
        return;
    };
    for (label, window) in [
        ("5-hour limit", usage.session),
        ("Weekly limit", usage.weekly),
    ] {
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
        .map_or("Not started".to_owned(), |s| {
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

/// An arrow icon, a bold rate and a muted word. No space after it: at the
/// end of a row that would widen the card as the numbers grow.
fn rate_chip(ui: &mut egui::Ui, icon: Icon, value: &str, what: &str) {
    let p = Palette::of(ui.ctx());
    icons::show(ui, icon, 16.0, p.faint);
    ui.label(
        egui::RichText::new(value)
            .font(theme::medium(15.0))
            .color(p.text),
    );
    ui.label(egui::RichText::new(what).color(p.muted));
}

fn series(samples: &[Snapshot], f: impl Fn(&Snapshot) -> Option<f32>) -> Vec<Option<f32>> {
    samples.iter().map(f).collect()
}

/// A small area chart of 0..1 values, newest at the right, with a caption.
/// The first series is filled; the others are lines.
fn sparkline(ui: &mut egui::Ui, lines: &[(Vec<Option<f32>>, Color32)], caption: &str) {
    const SLOTS: usize = 120;
    let p = Palette::of(ui.ctx());
    ui.add_space(10.0);
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), SPARK_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    // Faint guides at a quarter, half and three quarters.
    for t in [0.25, 0.5, 0.75] {
        let y = rect.top() + rect.height() * t;
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, p.border.gamma_multiply(0.6)),
        );
    }
    let dx = rect.width() / (SLOTS - 1) as f32;
    for (n, (values, color)) in lines.iter().enumerate() {
        let recent = &values[values.len().saturating_sub(SLOTS)..];
        let offset = SLOTS - recent.len();
        let points: Vec<Option<egui::Pos2>> = recent
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.map(|v| {
                    pos2(
                        rect.left() + (offset + i) as f32 * dx,
                        rect.bottom() - 1.0 - v.clamp(0.0, 1.0) * (rect.height() - 2.0),
                    )
                })
            })
            .collect();
        for run in points.split(Option::is_none) {
            let run: Vec<egui::Pos2> = run.iter().flatten().copied().collect();
            if run.len() < 2 {
                continue;
            }
            if n == 0 {
                // Fill under the line, one thin column per sample.
                let fill = color.gamma_multiply(0.14);
                for pair in run.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    painter.add(egui::Shape::convex_polygon(
                        vec![a, b, pos2(b.x, rect.bottom()), pos2(a.x, rect.bottom())],
                        fill,
                        Stroke::NONE,
                    ));
                }
            }
            painter.line(run, Stroke::new(1.5, *color));
        }
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(caption).small().color(p.faint));
}

fn number(v: f32) -> String {
    if v >= 10.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn ghz(mhz: f32) -> String {
    if mhz <= 0.0 {
        "—".to_owned()
    } else {
        format!("{:.2} GHz", mhz / 1000.0)
    }
}

/// Watts, or a dash when the power counters aren't readable (Settings →
/// Power readings turns them on).
fn watts(w: Option<f32>) -> String {
    w.map_or("—".to_owned(), |w| format!("{w:.0} W"))
}

/// "N/A" when the CPU is measured but nothing else is: this PC has no
/// whole-system sensor, and the package alone would just repeat CPU power.
fn pc_watts(s: &Snapshot) -> String {
    match (s.system_power_w, s.cpu_power_w) {
        (None, Some(_)) => "N/A".to_owned(),
        (w, _) => watts(w),
    }
}

fn celsius(c: Option<f32>) -> String {
    c.map_or("—".to_owned(), |c| format!("{c:.0} °C"))
}
