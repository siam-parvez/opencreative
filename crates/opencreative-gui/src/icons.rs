//! Line icons painted in code, so the app ships no icon assets.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Dashboard,
    Devices,
    Cooler,
    Settings,
    Cpu,
    Gpu,
    Memory,
    Storage,
    Network,
    Up,
    Down,
    Rotate,
    Help,
}

/// Allocates a `size` square and paints the icon in it.
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    response
}

/// Paints `icon` filling `rect` (a square works best).
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let stroke = Stroke::new((s / 14.0).max(1.4), color);
    // Coordinates in a -1..1 box around the center.
    let p = |x: f32, y: f32| pos2(c.x + x * s / 2.0, c.y + y * s / 2.0);
    let line = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], stroke);
    };
    let frame = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        painter.rect_stroke(
            Rect::from_min_max(p(x0, y0), p(x1, y1)),
            r * s / 2.0,
            stroke,
            egui::StrokeKind::Middle,
        );
    };
    let arc = |radius: f32, from: f32, to: f32| {
        let n = 24;
        let points = (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                p(radius * a.cos(), radius * a.sin())
            })
            .collect();
        painter.add(egui::Shape::line(points, stroke));
    };

    match icon {
        Icon::Dashboard => {
            arc(0.8, 0.75 * PI, 2.25 * PI);
            line(p(0.0, 0.1), p(0.38, -0.3));
            painter.circle_filled(p(0.0, 0.1), s * 0.06, color);
        }
        Icon::Devices => {
            frame(-0.85, -0.7, 0.85, 0.45, 0.12);
            line(p(0.0, 0.45), p(0.0, 0.75));
            line(p(-0.4, 0.78), p(0.4, 0.78));
        }
        Icon::Cooler => {
            frame(-0.8, -0.8, 0.8, 0.8, 0.25);
            frame(-0.45, -0.38, 0.45, 0.32, 0.06);
            painter.circle_filled(p(0.0, 0.58), s * 0.045, color);
        }
        Icon::Settings => {
            painter.circle_stroke(c, s * 0.2, stroke);
            arc(0.58, 0.0, TAU);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                line(
                    p(0.58 * a.cos(), 0.58 * a.sin()),
                    p(0.82 * a.cos(), 0.82 * a.sin()),
                );
            }
        }
        Icon::Cpu => {
            frame(-0.55, -0.55, 0.55, 0.55, 0.1);
            frame(-0.25, -0.25, 0.25, 0.25, 0.04);
            for t in [-0.3, 0.0, 0.3] {
                line(p(t, -0.55), p(t, -0.85));
                line(p(t, 0.55), p(t, 0.85));
                line(p(-0.55, t), p(-0.85, t));
                line(p(0.55, t), p(0.85, t));
            }
        }
        Icon::Gpu => {
            frame(-0.85, -0.5, 0.85, 0.5, 0.1);
            painter.circle_stroke(p(-0.25, 0.0), s * 0.17, stroke);
            line(p(0.3, -0.2), p(0.6, -0.2));
            line(p(0.3, 0.05), p(0.6, 0.05));
            line(p(-0.6, 0.5), p(-0.6, 0.75));
        }
        Icon::Memory => {
            frame(-0.85, -0.4, 0.85, 0.3, 0.06);
            for t in [-0.5, -0.17, 0.17, 0.5] {
                frame(t - 0.1, -0.2, t + 0.1, 0.08, 0.02);
                line(p(t, 0.3), p(t, 0.55));
            }
        }
        Icon::Storage => {
            frame(-0.8, -0.6, 0.8, 0.6, 0.15);
            line(p(-0.8, 0.15), p(0.8, 0.15));
            painter.circle_filled(p(0.5, 0.38), s * 0.05, color);
        }
        Icon::Network => {
            arc(0.78, 0.0, TAU);
            line(p(-0.78, 0.0), p(0.78, 0.0));
            // Meridian as a thin ellipse.
            let points = (0..=24)
                .map(|i| {
                    let a = i as f32 * TAU / 24.0;
                    p(0.32 * a.cos(), 0.78 * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        Icon::Up | Icon::Down => {
            arc(0.78, 0.0, TAU);
            let dir = if icon == Icon::Up { -1.0 } else { 1.0 };
            line(p(0.0, -0.4 * dir), p(0.0, 0.4 * dir));
            line(p(0.0, 0.4 * dir), p(-0.25, 0.15 * dir));
            line(p(0.0, 0.4 * dir), p(0.25, 0.15 * dir));
        }
        Icon::Rotate => {
            arc(0.65, -FRAC_PI_2, 1.4 * PI);
            let tip = p(0.0, -0.65);
            line(tip, p(-0.28, -0.88));
            line(tip, p(-0.28, -0.42));
        }
        Icon::Help => {
            painter.circle_filled(c, s * 0.48, color);
            painter.text(
                c + vec2(0.0, s * 0.02),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(s * 0.7),
                Color32::WHITE,
            );
        }
    }
}
