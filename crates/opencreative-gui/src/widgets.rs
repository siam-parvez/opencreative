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
    let font = theme::medium(14.0);
    let segment = vec2(segment_width(ui, options), 30.0);
    let mut changed = false;
    egui::Frame::new()
        .fill(p.control)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::SMALL_RADIUS)
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
                    let (rect, response) = ui.allocate_exact_size(segment, Sense::click());
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let painter = ui.painter();
                    if selected {
                        painter.rect(
                            rect,
                            theme::SMALL_RADIUS,
                            p.accent_soft,
                            Stroke::new(1.0, p.accent.gamma_multiply(0.6)),
                            egui::StrokeKind::Inside,
                        );
                    } else if response.hovered() {
                        painter.rect_filled(rect, theme::SMALL_RADIUS, p.raised);
                    }
                    let color = if selected {
                        p.accent
                    } else if response.hovered() {
                        p.text
                    } else {
                        p.muted
                    };
                    let font = if selected {
                        theme::semibold(14.0)
                    } else {
                        font.clone()
                    };
                    painter.text(rect.center(), Align2::CENTER_CENTER, *label, font, color);
                    if response.clicked() && !selected {
                        *value = *option;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// Equal segments, sized for the longest label.
fn segment_width<T>(ui: &egui::Ui, options: &[(T, &str)]) -> f32 {
    let font = theme::medium(14.0);
    let widest = options
        .iter()
        .map(|(_, label)| text_width(ui, label, &font))
        .fold(0.0, f32::max);
    (widest + 28.0).max(56.0)
}

/// The full width `segmented` takes for these options, for centering it.
pub fn segmented_width<T>(ui: &egui::Ui, options: &[(T, &str)]) -> f32 {
    let n = options.len() as f32;
    // Segments, the gaps between them, the inner margin and the border.
    n * segment_width(ui, options) + (n - 1.0) * 2.0 + 2.0 * 3.0 + 2.0
}

/// Fill and border of a selectable card or tile. Unselected cards ease
/// toward a raised fill and a brighter border while the pointer is over
/// them, so every picker answers the mouse the same way.
pub fn card_look(
    ui: &egui::Ui,
    id: egui::Id,
    selected: bool,
    hovered: bool,
    selected_fill: Color32,
    idle_fill: Color32,
) -> (Color32, Stroke) {
    let p = Palette::of(ui.ctx());
    let t = ui
        .ctx()
        .animate_bool_responsive(id.with("hover"), hovered && !selected);
    if selected {
        (selected_fill, Stroke::new(2.0, p.accent))
    } else {
        (
            lerp_color(idle_fill, p.raised, t),
            Stroke::new(1.0, lerp_color(p.border, p.faint, t)),
        )
    }
}

/// A row of picture tiles, one per option, like the font and skin pickers.
/// `paint` draws an option's preview into the top of its tile. Returns true
/// if the choice changed.
pub fn preview_tiles<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, &str)],
    size: egui::Vec2,
    paint: impl Fn(&egui::Painter, Rect, T),
) -> bool {
    let p = Palette::of(ui.ctx());
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for &(option, label) in options {
            let selected = *value == option;
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            let (fill, border) = card_look(
                ui,
                response.id,
                selected,
                response.hovered(),
                p.raised,
                p.card,
            );
            let painter = ui.painter();
            painter.rect(
                rect,
                theme::SMALL_RADIUS,
                fill,
                border,
                egui::StrokeKind::Inside,
            );
            let label_h = 28.0;
            let preview = Rect::from_min_max(
                rect.min + vec2(9.0, 9.0),
                pos2(rect.max.x - 9.0, rect.max.y - label_h),
            );
            paint(painter, preview, option);
            let font = if selected {
                theme::semibold(13.0)
            } else {
                theme::medium(13.0)
            };
            painter.text(
                pos2(rect.center().x, rect.max.y - label_h / 2.0),
                Align2::CENTER_CENTER,
                label,
                font,
                if selected { p.accent } else { p.text },
            );
            if response.clicked() && !selected {
                *value = option;
                changed = true;
            }
        }
    });
    changed
}

/// A miniature app window in `p`'s colors: sidebar and two cards.
pub fn paint_mini_window(painter: &egui::Painter, rect: Rect, p: &Palette) {
    let r = theme::SMALL_RADIUS;
    let edge = Stroke::new(1.0, p.border);
    painter.rect(rect, r, p.bg, edge, egui::StrokeKind::Inside);
    let sidebar = Rect::from_min_size(rect.min, vec2(rect.width() * 0.28, rect.height()));
    let sidebar_corners = CornerRadius {
        nw: r,
        sw: r,
        ne: 0,
        se: 0,
    };
    painter.rect(
        sidebar,
        sidebar_corners,
        p.sidebar,
        edge,
        egui::StrokeKind::Inside,
    );
    // Navigation lines, the first one selected.
    for i in 0..3 {
        let line = Rect::from_min_size(
            pos2(sidebar.left() + 5.0, sidebar.top() + 8.0 + i as f32 * 7.0),
            vec2(sidebar.width() - 12.0, 3.0),
        );
        painter.rect_filled(line, 1.5, if i == 0 { p.accent } else { p.faint });
    }
    let content = Rect::from_min_max(
        pos2(sidebar.right() + 6.0, rect.top() + 6.0),
        rect.max - vec2(6.0, 6.0),
    );
    let card_h = (content.height() - 5.0) / 2.0;
    for i in 0..2 {
        let card = Rect::from_min_size(
            pos2(content.left(), content.top() + i as f32 * (card_h + 5.0)),
            vec2(content.width(), card_h),
        );
        painter.rect(card, r, p.card, edge, egui::StrokeKind::Inside);
        let title = Rect::from_min_size(card.min + vec2(5.0, 5.0), vec2(card.width() * 0.5, 3.0));
        painter.rect_filled(title, 1.5, p.text.gamma_multiply(0.7));
        let body = Rect::from_min_size(card.min + vec2(5.0, 11.0), vec2(card.width() * 0.7, 3.0));
        painter.rect_filled(body, 1.5, p.muted.gamma_multiply(0.6));
    }
}

/// A quiet "go there" link: accent text and an arrow that nudges right on
/// hover, over a faint raised background. For moving to another page, where
/// a filled button would shout.
pub fn arrow_link(ui: &mut egui::Ui, label: &str) -> Response {
    let p = Palette::of(ui.ctx());
    let font = theme::medium(14.0);
    let pad = 10.0;
    let arrow = 14.0;
    // The text lines up with the content above; only the hover background
    // reaches out into the left padding.
    let size = vec2(text_width(ui, label, &font) + 8.0 + arrow + pad, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered());
    let painter = ui.painter();
    let fill = if response.is_pointer_button_down_on() {
        p.track
    } else {
        p.raised.gamma_multiply(t)
    };
    let background = Rect::from_min_max(rect.min - vec2(pad, 0.0), rect.max);
    painter.rect_filled(background, theme::SMALL_RADIUS, fill);
    let text = painter.text(
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        p.accent,
    );
    // A right arrow: shaft and head, sliding 3 points on hover.
    let y = rect.center().y;
    let x0 = text.right() + 8.0 + 3.0 * t;
    let x1 = x0 + arrow - 3.0;
    let stroke = Stroke::new(1.6, p.accent);
    painter.line_segment([pos2(x0, y), pos2(x1, y)], stroke);
    painter.line_segment([pos2(x1 - 4.5, y - 4.5), pos2(x1, y)], stroke);
    painter.line_segment([pos2(x1 - 4.5, y + 4.5), pos2(x1, y)], stroke);
    response
}

/// A small rounded choice, for quick picks: outlined, or accent-tinted when
/// it matches the current setting.
pub fn pill(ui: &mut egui::Ui, label: &str, selected: bool) -> Response {
    let p = Palette::of(ui.ctx());
    let font = theme::medium(13.0);
    let size = vec2(text_width(ui, label, &font) + 24.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered());
    let (fill, stroke, text) = if selected {
        (p.accent_soft, p.accent.gamma_multiply(0.6), p.accent)
    } else {
        (
            p.raised.gamma_multiply(t),
            lerp_color(p.border, p.faint, t),
            lerp_color(p.muted, p.text, t),
        )
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(14),
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, font, text);
    response
}

/// A small rounded status label, e.g. "Connected".
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .corner_radius(theme::RADIUS)
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

/// Units a reading may end in, longest first.
const UNITS: [&str; 13] = [
    "KB/s", "MB/s", "GB/s", "TB/s", "B/s", "GHz", "°C", "GB", "MB", "KB", "TB", "W", "%",
];

/// Splits "1.82 GHz" into ("1.82", "GHz"); text without a known unit
/// ("N/A", "—") is all number.
pub fn split_unit(value: &str) -> (&str, &str) {
    for unit in UNITS {
        if let Some(number) = value.strip_suffix(unit) {
            let number = number.trim_end();
            if number.len() < value.len() - unit.len() || unit == "%" {
                return (number, unit);
            }
        }
    }
    (value, "")
}

/// Slots for a reading: the number right-aligned in the first, the unit in
/// the second, so only the digits move as the value changes.
#[derive(Debug, Clone, Copy)]
pub struct Slots {
    pub number: f32,
    pub unit: f32,
}

impl Slots {
    /// Room for `template` (e.g. "8.88 GHz": the longest number expected,
    /// and the widest unit) in `font`.
    pub fn fit(ui: &egui::Ui, template: &str, font: &egui::FontId) -> Self {
        let (number, unit) = split_unit(template);
        let unit_w = if unit.is_empty() {
            0.0
        } else {
            text_width(ui, unit, font)
        };
        Self {
            number: tabular_width(ui, number, font),
            unit: unit_w,
        }
    }

    pub fn width(self) -> f32 {
        if self.unit > 0.0 {
            self.number + UNIT_GAP + self.unit
        } else {
            self.number
        }
    }
}

const UNIT_GAP: f32 = 4.0;

fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x
}

/// Every digit gets the widest digit's room, like a font's tabular figures:
/// "1.82" and "4.55" take the same space.
fn digit_width(ui: &egui::Ui, font: &egui::FontId) -> f32 {
    ('0'..='9')
        .map(|d| text_width(ui, &d.to_string(), font))
        .fold(0.0, f32::max)
}

fn tabular_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    let digit = digit_width(ui, font);
    text.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                digit
            } else {
                text_width(ui, &c.to_string(), font)
            }
        })
        .sum()
}

/// Paints `text` with tabular digits, starting at `x`.
fn paint_tabular(ui: &egui::Ui, x: f32, y: f32, text: &str, font: &egui::FontId, color: Color32) {
    let digit = digit_width(ui, font);
    let mut x = x;
    for c in text.chars() {
        let s = c.to_string();
        if c.is_ascii_digit() {
            ui.painter().text(
                pos2(x + digit / 2.0, y),
                Align2::CENTER_CENTER,
                &s,
                font.clone(),
                color,
            );
            x += digit;
        } else {
            ui.painter()
                .text(pos2(x, y), Align2::LEFT_CENTER, &s, font.clone(), color);
            x += text_width(ui, &s, font);
        }
    }
}

/// A reading in its [`Slots`]: tabular digits right-aligned against a unit
/// that stays put (muted). Text with no unit ("N/A", "—") starts at the left.
pub fn reading(ui: &mut egui::Ui, value: &str, font: egui::FontId, color: Color32, slots: Slots) {
    let p = Palette::of(ui.ctx());
    let (number, unit) = split_unit(value);
    let height = ui.fonts_mut(|f| f.row_height(&font));
    let (rect, _) = ui.allocate_exact_size(vec2(slots.width(), height), Sense::hover());
    let y = rect.center().y;
    if unit.is_empty() {
        ui.painter().text(
            pos2(rect.left(), y),
            Align2::LEFT_CENTER,
            number,
            font,
            color,
        );
        return;
    }
    let number_right = rect.left() + slots.number;
    let start = number_right - tabular_width(ui, number, &font);
    paint_tabular(ui, start, y, number, &font, color);
    ui.painter().text(
        pos2(number_right + UNIT_GAP, y),
        Align2::LEFT_CENTER,
        unit,
        font,
        p.muted,
    );
}

/// An accent reading over a muted label, as in the dashboard cards.
pub fn stat(ui: &mut egui::Ui, value: &str, label: &str, template: &str) {
    let p = Palette::of(ui.ctx());
    let slots = Slots::fit(ui, template, &theme::medium(16.0));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        reading(ui, value, theme::medium(16.0), p.text, slots);
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

    // Placed by the digits' ink, not the font's line box (the numeric font
    // has room above its digits). The ring is open at the bottom and the
    // caption sits below, so the number goes a little above the true center
    // to look centered.
    let middle = center.y - radius * 0.1;
    let value = painter.layout_no_wrap(value.to_owned(), theme::numeric(diameter * 0.2), p.text);
    let ink = value.mesh_bounds;
    let unit = (!unit.is_empty()).then(|| {
        painter.layout_no_wrap(unit.to_owned(), theme::regular(diameter * 0.085), p.muted)
    });
    let gap = 4.0;
    let unit_w = unit.as_ref().map_or(0.0, |u| gap + u.mesh_bounds.width());
    let left = center.x - (ink.width() + unit_w) / 2.0;
    let value_pos = pos2(left - ink.min.x, middle - ink.center().y);
    // The unit sits on the digits' baseline.
    let baseline = middle + ink.height() / 2.0;
    painter.galley(value_pos, value, p.text);
    if let Some(unit) = unit {
        let unit_ink = unit.mesh_bounds;
        let pos = pos2(
            left + ink.width() + gap - unit_ink.min.x,
            baseline - unit_ink.max.y,
        );
        painter.galley(pos, unit, p.muted);
    }
    painter.text(
        pos2(center.x, center.y + radius * 0.42),
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

/// Blends two colors, alpha included, so easing from transparent fades in
/// instead of starting black. Color32 is premultiplied, so every channel
/// blends linearly.
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blending_from_transparent_stays_transparent() {
        let raised = Color32::from_rgb(0xee, 0xf0, 0xf3);
        assert_eq!(
            lerp_color(Color32::TRANSPARENT, raised, 0.0),
            Color32::TRANSPARENT
        );
        assert_eq!(lerp_color(Color32::TRANSPARENT, raised, 1.0), raised);
        assert_eq!(lerp_color(Color32::BLACK, Color32::WHITE, 0.5).a(), 255);
    }

    #[test]
    fn units_split_off_numbers() {
        assert_eq!(split_unit("1.82 GHz"), ("1.82", "GHz"));
        assert_eq!(split_unit("41 °C"), ("41", "°C"));
        assert_eq!(split_unit("8.3 KB/s"), ("8.3", "KB/s"));
        assert_eq!(split_unit("512 B/s"), ("512", "B/s"));
        assert_eq!(split_unit("1.2 / 8.0 GB"), ("1.2 / 8.0", "GB"));
        assert_eq!(split_unit("N/A"), ("N/A", ""));
        assert_eq!(split_unit("—"), ("—", ""));
    }

    #[test]
    fn formats_sizes_and_rates() {
        assert_eq!(bytes(0.0), "0 B");
        assert_eq!(bytes(1536.0), "1.5 KB");
        assert_eq!(bytes(19.6 * 1024.0 * 1024.0 * 1024.0), "19.6 GB");
        assert_eq!(bytes(150.0 * 1024.0 * 1024.0 * 1024.0), "150 GB");
        assert_eq!(rate(8.3 * 1024.0), "8.3 KB/s");
    }
}

/// A pick in a [`swatches`] row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Swatch {
    /// Every bar its own color, by pitch.
    Rainbow,
    Color([u8; 3]),
}

/// A row of round color swatches: an optional rainbow, the presets, and a
/// custom one that opens a color picker. Returns the new pick when it
/// changes.
pub fn swatches(
    ui: &mut egui::Ui,
    id: &str,
    current: Swatch,
    presets: &[[u8; 3]],
    rainbow: bool,
) -> Option<Swatch> {
    let p = Palette::of(ui.ctx());
    let mut picked = None;
    let size = vec2(26.0, 26.0);
    let ring = |ui: &egui::Ui, rect: Rect, selected: bool, hovered: bool| {
        let (width, color) = match (selected, hovered) {
            (true, _) => (2.0, p.accent),
            (false, true) => (1.5, p.muted),
            (false, false) => (1.0, p.border),
        };
        ui.painter()
            .circle_stroke(rect.center(), 12.0, Stroke::new(width, color));
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if rainbow {
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            paint_rainbow(ui, rect.center(), 10.0);
            ring(ui, rect, current == Swatch::Rainbow, response.hovered());
            if response
                .on_hover_text("Rainbow: each bar by its pitch")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                picked = Some(Swatch::Rainbow);
            }
        }
        for &rgb in presets {
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            ui.painter().circle_filled(rect.center(), 10.0, fill);
            ring(ui, rect, current == Swatch::Color(rgb), response.hovered());
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                picked = Some(Swatch::Color(rgb));
            }
        }

        // Custom: shows the chosen color when it isn't a preset, and
        // remembers the last one picked.
        let memory = egui::Id::new(("custom-swatch", id));
        let is_custom = matches!(current, Swatch::Color(c) if !presets.contains(&c));
        let mut custom: [u8; 3] = match current {
            Swatch::Color(c) if is_custom => c,
            _ => ui
                .ctx()
                .data(|d| d.get_temp(memory))
                .unwrap_or([0x80, 0x80, 0x80]),
        };
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let fill = Color32::from_rgb(custom[0], custom[1], custom[2]);
        ui.painter().circle_filled(rect.center(), 10.0, fill);
        // A plus marks it as the custom picker.
        let plus = if (custom.iter().map(|&v| u32::from(v)).sum::<u32>()) > 380 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        let c = rect.center();
        ui.painter().line_segment(
            [c - vec2(4.0, 0.0), c + vec2(4.0, 0.0)],
            Stroke::new(1.5, plus),
        );
        ui.painter().line_segment(
            [c - vec2(0.0, 4.0), c + vec2(0.0, 4.0)],
            Stroke::new(1.5, plus),
        );
        ring(ui, rect, is_custom, response.hovered());
        let response = response
            .on_hover_text("Custom color")
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let mut color = Color32::from_rgb(custom[0], custom[1], custom[2]);
                if egui::color_picker::color_picker_color32(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                ) {
                    custom = [color.r(), color.g(), color.b()];
                    picked = Some(Swatch::Color(custom));
                }
            });
        if response.clicked() && !is_custom && picked.is_none() {
            picked = Some(Swatch::Color(custom));
        }
        ui.ctx().data_mut(|d| d.insert_temp(memory, custom));
    });
    picked.filter(|&new| new != current)
}

/// A color wheel: wedges going round the hues.
fn paint_rainbow(ui: &egui::Ui, center: egui::Pos2, radius: f32) {
    const WEDGES: usize = 24;
    for i in 0..WEDGES {
        let a0 = std::f32::consts::TAU * i as f32 / WEDGES as f32;
        let a1 = std::f32::consts::TAU * (i + 1) as f32 / WEDGES as f32;
        let hsva = egui::ecolor::Hsva::new(i as f32 / WEDGES as f32, 1.0, 1.0, 1.0);
        let points = vec![
            center,
            center + radius * vec2(a0.cos(), a0.sin()),
            center + radius * vec2(a1.cos(), a1.sin()),
        ];
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            Color32::from(hsva),
            Stroke::NONE,
        ));
    }
}
