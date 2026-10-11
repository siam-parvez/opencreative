//! Custom widgets: toggle switch, segmented control, gauge, usage bar and
//! labelled stats, styled after the app theme.

use std::f32::consts::PI;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, pos2, vec2,
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
                    let text = egui::RichText::new(*label).color(if selected {
                        Color32::WHITE
                    } else {
                        p.text
                    });
                    let button = egui::Button::new(text)
                        .fill(if selected {
                            p.accent
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
        ui.label(
            egui::RichText::new(value)
                .font(theme::semibold(17.0))
                .color(p.accent),
        );
        ui.label(
            egui::RichText::new(label)
                .font(theme::regular(12.0))
                .color(p.muted),
        );
    });
}

/// A 270-degree gauge, open at the bottom, with a big centered value.
pub fn gauge(
    ui: &mut egui::Ui,
    diameter: f32,
    fraction: f32,
    value: &str,
    unit: &str,
    caption: &str,
    ends: (&str, &str),
) {
    let p = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(diameter, diameter * 0.86), Sense::hover());
    let painter = ui.painter();
    let center = pos2(rect.center().x, rect.top() + diameter / 2.0);
    let radius = diameter / 2.0 - 8.0;
    let (start, sweep) = (0.75 * PI, 1.5 * PI);
    let arc = |to: f32, color: Color32, width: f32| {
        let n = 64;
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
    arc(1.0, p.track, 9.0);
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.005 {
        arc(fraction, p.accent, 9.0);
    }

    painter.text(
        center - vec2(0.0, diameter * 0.17),
        Align2::CENTER_CENTER,
        caption,
        theme::regular(12.0),
        p.muted,
    );
    let value_font = theme::medium(diameter * 0.27);
    let unit_font = theme::regular(diameter * 0.09);
    let value_w = painter
        .layout_no_wrap(value.to_owned(), value_font.clone(), p.text)
        .size()
        .x;
    let unit_w = painter
        .layout_no_wrap(unit.to_owned(), unit_font.clone(), p.muted)
        .size()
        .x;
    let left = center.x - (value_w + 3.0 + unit_w) / 2.0;
    let baseline_y = center.y + diameter * 0.05;
    painter.text(
        pos2(left, baseline_y),
        Align2::LEFT_CENTER,
        value,
        value_font,
        p.text,
    );
    painter.text(
        pos2(left + value_w + 3.0, baseline_y + diameter * 0.07),
        Align2::LEFT_CENTER,
        unit,
        unit_font,
        p.muted,
    );
    let end_y = center.y + radius * 0.72;
    painter.text(
        pos2(center.x - radius * 0.95, end_y),
        Align2::RIGHT_CENTER,
        ends.0,
        theme::regular(11.0),
        p.faint,
    );
    painter.text(
        pos2(center.x + radius * 0.95, end_y),
        Align2::LEFT_CENTER,
        ends.1,
        theme::regular(11.0),
        p.faint,
    );
}

/// A usage bar with diagonal hatching on the used part.
pub fn usage_bar(ui: &mut egui::Ui, fraction: f32, height: f32) {
    let p = Palette::of(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect.expand(1.0));
    let radius = CornerRadius::same(3);
    painter.rect_filled(rect, radius, p.card);
    let used = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    if used.width() > 0.5 {
        let fill = p.accent.gamma_multiply(0.55);
        painter.rect_filled(used, radius, fill);
        let hatch = painter.with_clip_rect(used);
        let stroke = Stroke::new(1.0, p.accent);
        let step = 6.0;
        let mut x = used.left() - used.height();
        while x < used.right() {
            hatch.line_segment(
                [pos2(x, used.bottom()), pos2(x + used.height(), used.top())],
                stroke,
            );
            x += step;
        }
    }
    painter.rect_stroke(rect, radius, Stroke::new(1.0, p.accent), StrokeKind::Inside);
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
