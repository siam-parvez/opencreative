//! Sensor history for the dashboard: the latest reading, a two-minute chart
//! and every temperature sensor found.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, vec2};
use opencreative_sensors::{Sensors, Snapshot};

const SAMPLE_EVERY: Duration = Duration::from_secs(1);
/// Two minutes at one sample per second.
const KEEP: usize = 120;
const CHART_HEIGHT: f32 = 220.0;

/// A plotted value: label, color, unit and how to read it from a snapshot.
struct Series {
    label: &'static str,
    unit: &'static str,
    color: Color32,
    value: fn(&Snapshot) -> Option<f32>,
}

const SERIES: [Series; 4] = [
    Series {
        label: "CPU temp",
        unit: "°C",
        color: Color32::from_rgb(0x22, 0xd3, 0xee),
        value: |s| s.cpu_temp_c,
    },
    Series {
        label: "CPU load",
        unit: "%",
        color: Color32::from_rgb(0xa7, 0x8b, 0xfa),
        value: |s| Some(s.cpu_load_pct),
    },
    Series {
        label: "GPU temp",
        unit: "°C",
        color: Color32::from_rgb(0xfb, 0xbf, 0x24),
        value: |s| s.gpu.temp_c,
    },
    Series {
        label: "RAM",
        unit: "%",
        color: Color32::from_rgb(0x4a, 0xde, 0x80),
        value: |s| Some(s.mem_used_pct),
    },
];

pub struct SensorHistory {
    sensors: Sensors,
    samples: VecDeque<Snapshot>,
    temperatures: Vec<(String, Option<f32>)>,
    next_sample: Instant,
}

impl SensorHistory {
    pub fn new() -> Self {
        Self {
            sensors: Sensors::new(),
            samples: VecDeque::with_capacity(KEEP),
            temperatures: Vec::new(),
            next_sample: Instant::now(),
        }
    }

    /// Samples if one is due; returns how long until the next.
    pub fn tick(&mut self) -> Duration {
        let now = Instant::now();
        if now >= self.next_sample {
            if self.samples.len() == KEEP {
                self.samples.pop_front();
            }
            self.samples.push_back(self.sensors.read());
            self.temperatures = self.sensors.all_temperatures();
            self.next_sample = now + SAMPLE_EVERY;
        }
        self.next_sample.saturating_duration_since(now)
    }

    /// The most recent reading, if one has been taken.
    pub fn latest(&self) -> Option<&Snapshot> {
        self.samples.back()
    }

    /// Recent readings, oldest first.
    pub fn samples(&self) -> Vec<Snapshot> {
        self.samples.iter().cloned().collect()
    }

    pub fn temperatures(&self) -> &[(String, Option<f32>)] {
        &self.temperatures
    }

    /// Legend with current values, then the chart.
    pub fn chart_with_legend(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for series in &SERIES {
                let latest = self.samples.back().and_then(series.value);
                let text = match latest {
                    Some(v) => format!("{} {v:.0}{}", series.label, series.unit),
                    None => format!("{} --", series.label),
                };
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, series.color);
                ui.label(text);
                ui.add_space(10.0);
            }
        });
        self.chart(ui);
    }

    /// Line chart on a fixed 0-100 scale, which fits both °C and %.
    fn chart(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, CHART_HEIGHT), Sense::hover());
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(rect, 6.0, visuals.extreme_bg_color);

        let plot = rect.shrink2(vec2(28.0, 10.0)).translate(vec2(12.0, 0.0));
        let y_of = |v: f32| plot.bottom() - v.clamp(0.0, 100.0) / 100.0 * plot.height();
        let grid = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3));
        for v in [0.0, 25.0, 50.0, 75.0, 100.0] {
            let y = y_of(v);
            painter.hline(plot.x_range(), y, grid);
            painter.text(
                Pos2::new(rect.left() + 4.0, y),
                Align2::LEFT_CENTER,
                format!("{v:.0}"),
                FontId::proportional(11.0),
                visuals.weak_text_color(),
            );
        }

        // Newest sample at the right edge.
        let dx = plot.width() / (KEEP - 1) as f32;
        let offset = KEEP - self.samples.len();
        for series in &SERIES {
            let mut run: Vec<Pos2> = Vec::new();
            for (i, sample) in self.samples.iter().enumerate() {
                match (series.value)(sample) {
                    Some(v) => run.push(Pos2::new(plot.left() + (offset + i) as f32 * dx, y_of(v))),
                    // A missing reading breaks the line rather than faking a value.
                    None => flush(&painter, &mut run, series.color),
                }
            }
            flush(&painter, &mut run, series.color);
        }
    }
}

fn flush(painter: &egui::Painter, run: &mut Vec<Pos2>, color: Color32) {
    if run.len() > 1 {
        painter.line(std::mem::take(run), Stroke::new(2.0, color));
    }
    run.clear();
}
