//! Custom widgets: toggle switch, segmented control, gauge, usage bar and
//! labelled stats, styled after the app theme.

use std::f32::consts::PI;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, pos2, vec2,
};

use crate::theme::{self, Palette};

/// An iOS-style on/off switch.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> Response {
    let size = vec2(38.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let p = Palette::of(ui.ctx());
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let fill = lerp_color(p.track, p.accent, t);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), fill);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    painter.circle_filled(pos2(x, rect.center().y), 8.5, Color32::WHITE);
    response
}

/// A row of mutually exclusive options. Returns true if the choice changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let p = Palette::of(ui.ctx());
    let mut changed = false;
    egui::Frame::new()
        .fill(p.control)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(8)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                // Inside a right-to-left row, add options in reverse so they
                // still read left to right.
                let mut ordered: Vec<_> = options.iter().collect();
                if ui.layout().main_dir() == egui::Direction::RightToLeft {
                    ordered.reverse();
                }
                for (option, label) in ordered {
                    let selected = *value == *option;
                    let text =
                        egui::RichText::new(*label).color(if selected { p.text } else { p.muted });
                    let button = egui::Button::new(text)
                        .fill(if selected {
                            p.raised
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE)
                        .corner_radius(6);
                    if ui.add(button).clicked() && !selected {
                        *value = *option;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A small rounded status label, e.g. "Connected".
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().color(color));
        });
}

/// A small muted heading inside a card.
pub fn section(ui: &mut egui::Ui, text: &str) {
    let p = Palette::of(ui.ctx());
    ui.label(
        egui::RichText::new(text)
            .font(theme::semibold(15.0))
            .color(p.text),
    );
}

/// An accent value over a muted label, as in the dashboard cards.
pub fn stat(ui: &mut egui::Ui, value: &str, label: &str) {
    let p = Palette::of(ui.ctx());
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        // Never wrap: a stat reads as one value and one label.
        ui.add(
            egui::Label::new(
                egui::RichText::new(value)
                    .font(theme::medium(16.0))
                    .color(p.text),
            )
            .extend(),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(label)
                    .font(theme::regular(12.0))
                    .color(p.muted),
            )
            .extend(),
        );
    });
}

/// A 270-degree gauge, open at the bottom. The value sits on the circle's
/// center with the caption below it, inside the arc.
pub fn gauge(
    ui: &mut egui::Ui,
    diameter: f32,
    fraction: f32,
    value: &str,
    unit: &str,
    caption: &str,
    color: Color32,
) {
    let p = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(diameter, diameter * 0.88), Sense::hover());
    let painter = ui.painter();
    let center = pos2(rect.center().x, rect.top() + diameter / 2.0);
    let width = (diameter * 0.055).max(6.0);
    let radius = diameter / 2.0 - width;
    let (start, sweep) = (0.75 * PI, 1.5 * PI);
    let arc = |to: f32, color: Color32| {
        let n = 96;
        let points: Vec<_> = (0..=n)
            .map(|i| {
                let a = start + sweep * to * i as f32 / n as f32;
                center + radius * vec2(a.cos(), a.sin())
            })
            .collect();
        if let (Some(first), Some(last)) = (points.first(), points.last()) {
            // Round caps.
            painter.circle_filled(*first, width / 2.0, color);
            painter.circle_filled(*last, width / 2.0, color);
        }
        painter.add(egui::Shape::line(points, Stroke::new(width, color)));
    };
    arc(1.0, p.track);
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.005 {
        arc(fraction, color);
    }
    if fraction > 0.04 {
        // A soft glow at the tip.
        let a = start + sweep * fraction;
        let tip = center + radius * vec2(a.cos(), a.sin());
        painter.circle_filled(tip, width * 1.1, color.gamma_multiply(0.18));
    }

    let value_font = theme::numeric(diameter * 0.2);
    let unit_font = theme::regular(diameter * 0.085);
    let value_w = painter
        .layout_no_wrap(value.to_owned(), value_font.clone(), p.text)
        .size()
        .x;
    let unit_w = if unit.is_empty() {
        0.0
    } else {
        painter
            .layout_no_wrap(unit.to_owned(), unit_font.clone(), p.muted)
            .size()
            .x
            + 4.0
    };
    let left = center.x - (value_w + unit_w) / 2.0;
    painter.text(
        pos2(left, center.y),
        Align2::LEFT_CENTER,
        value,
        value_font,
        p.text,
    );
    if !unit.is_empty() {
        painter.text(
            pos2(left + value_w + 4.0, center.y + diameter * 0.03),
            Align2::LEFT_CENTER,
            unit,
            unit_font,
            p.muted,
        );
    }
    painter.text(
        pos2(center.x, center.y + radius * 0.5),
        Align2::CENTER_CENTER,
        caption,
        theme::regular(12.0),
        p.muted,
    );
}

/// A slim usage bar in the accent color.
pub fn usage_bar(ui: &mut egui::Ui, fraction: f32, height: f32) {
    let accent = Palette::of(ui.ctx()).accent;
    usage_bar_colored(ui, fraction, height, accent);
}

/// A slim, rounded usage bar in a given color.
pub fn usage_bar_colored(ui: &mut egui::Ui, fraction: f32, height: f32, color: Color32) {
    let p = Palette::of(ui.ctx());
    let width = ui.available_width();
    let height = height.min(8.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same((height / 2.0) as u8);
    painter.rect_filled(rect, radius, p.track);
    let used = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    if used.width() >= height {
        painter.rect_filled(used, radius, color);
    } else if used.width() > 0.5 {
        painter.circle_filled(
            rect.left_center() + vec2(height / 2.0, 0.0),
            height / 2.0,
            color,
        );
    }
}

/// Formats bytes per second, e.g. `8.3 KB/s`.
pub fn rate(bytes_per_sec: f64) -> String {
    format!("{}/s", bytes(bytes_per_sec))
}

/// Formats a byte count with a binary unit, e.g. `19.6 GB`.
pub fn bytes(n: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n.max(0.0);
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 || v >= 100.0 {
        format!("{v:.0} {}", UNITS[unit])
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_and_rates() {
        assert_eq!(bytes(0.0), "0 B");
        assert_eq!(bytes(1536.0), "1.5 KB");
        assert_eq!(bytes(19.6 * 1024.0 * 1024.0 * 1024.0), "19.6 GB");
        assert_eq!(bytes(150.0 * 1024.0 * 1024.0 * 1024.0), "150 GB");
        assert_eq!(rate(8.3 * 1024.0), "8.3 KB/s");
    }
}
