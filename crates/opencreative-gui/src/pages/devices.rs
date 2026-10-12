//! Devices: supported coolers found on USB, pictured with their live screen.

use eframe::egui::{self, Color32, Rect, Sense, pos2, vec2};

use crate::icons::{self, Icon};
use crate::pages::header;
use crate::prefs::{Finish, Model};
use crate::theme::{self, Palette};
use crate::widgets::{chip, primary_button, segmented};
use crate::{App, Page};

/// A product picture, its aspect ratio, and where the pump's screen sits in
/// it as fractions of the image. Both finishes of a model share one framing.
struct Picture {
    source: egui::ImageSource<'static>,
    aspect: f32,
    screen: Rect,
}

fn picture_of(model: Model, finish: Finish) -> Picture {
    use egui::include_image as png;
    let (aspect, screen) = match model {
        Model::Lm240 => (
            900.0 / 400.0,
            Rect::from_min_max(pos2(0.627, 0.455), pos2(0.774, 0.710)),
        ),
        Model::Lm360 => (
            900.0 / 367.0,
            Rect::from_min_max(pos2(0.679, 0.505), pos2(0.800, 0.738)),
        ),
    };
    let source = match (model, finish) {
        (Model::Lm240, Finish::Black) => png!("../../../../assets/devices/lm240-black.png"),
        (Model::Lm240, Finish::White) => png!("../../../../assets/devices/lm240-white.png"),
        (Model::Lm360, Finish::Black) => png!("../../../../assets/devices/lm360-black.png"),
        (Model::Lm360, Finish::White) => png!("../../../../assets/devices/lm360-white.png"),
    };
    Picture {
        source,
        aspect,
        screen,
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    header(
        ui,
        "Devices",
        Some("DeepCool coolers connected to this computer"),
    );

    if app.devices.is_empty() {
        theme::card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                picture(app, ui, ui.available_width().min(420.0), false);
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("No supported cooler found")
                        .font(theme::semibold(17.0))
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new(
                        "Plug in a DeepCool LM-series cooler's USB cable. \
                         If it is connected, check USB access in Settings.",
                    )
                    .color(p.muted),
                );
                ui.add_space(10.0);
                if ui.button("Open Settings").clicked() {
                    app.page = Page::Settings;
                }
                ui.add_space(12.0);
            });
        });
        ui.add_space(14.0);
    }

    let rows: Vec<_> = app
        .devices
        .iter()
        .map(|d| (d.name, d.width, d.height, d.serial.clone()))
        .collect();
    for (name, width, height, serial) in rows {
        let connected = app.device.as_deref() == Some(name);
        theme::card(ui, |ui| {
            // Picture beside the details when there's room, above them when not.
            const DETAILS_W: f32 = 300.0;
            const GAP: f32 = 24.0;
            let available = ui.available_width();
            let side_by_side = available >= 360.0 + GAP + DETAILS_W;
            let details = |app: &mut App, ui: &mut egui::Ui| {
                let (text, color) = if connected {
                    ("Connected", p.good)
                } else {
                    ("Found, not in use", p.faint)
                };
                chip(ui, text, color);
                ui.add_space(6.0);
                let title = app.device_title(name);
                ui.label(
                    egui::RichText::new(title)
                        .font(theme::semibold(22.0))
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new(format!("{width} × {height} IPS screen · USB"))
                        .color(p.muted),
                );
                if let Some(serial) = &serial {
                    ui.label(
                        egui::RichText::new(format!("Serial {serial}"))
                            .small()
                            .color(p.faint),
                    );
                }
                ui.add_space(14.0);
                ui.label(
                    egui::RichText::new("Model and finish (for the picture)")
                        .small()
                        .color(p.muted),
                );
                ui.horizontal_wrapped(|ui| {
                    let mut model = app.prefs.model;
                    let mut finish = app.prefs.finish;
                    let changed = segmented(
                        ui,
                        &mut model,
                        &[(Model::Lm240, "LM240"), (Model::Lm360, "LM360")],
                    ) | segmented(
                        ui,
                        &mut finish,
                        &[(Finish::Black, "Black"), (Finish::White, "White")],
                    );
                    if changed {
                        app.prefs.model = model;
                        app.prefs.finish = finish;
                        app.save_prefs();
                    }
                });
                ui.add_space(14.0);
                if primary_button(ui, "Customize screen").clicked() {
                    app.page = Page::Device;
                }
            };
            if side_by_side {
                let picture_w = (available - GAP - DETAILS_W).min(640.0);
                ui.horizontal(|ui| {
                    picture(app, ui, picture_w, connected);
                    ui.add_space(GAP);
                    ui.vertical(|ui| {
                        ui.add_space(18.0);
                        details(app, ui);
                    });
                });
            } else {
                ui.vertical_centered(|ui| picture(app, ui, available.min(560.0), connected));
                ui.add_space(12.0);
                details(app, ui);
            }
        });
        ui.add_space(14.0);
    }

    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            icons::show(ui, Icon::Help, 16.0, p.muted);
            ui.label(
                egui::RichText::new("Supported models")
                    .font(theme::semibold(15.0))
                    .color(p.text),
            );
        });
        ui.add_space(4.0);
        for spec in opencreative_device::SUPPORTED {
            ui.label(
                egui::RichText::new(format!(
                    "{}  ·  USB {:04x}:{:04x}  ·  {} × {}",
                    spec.name, spec.vendor_id, spec.product_id, spec.width, spec.height
                ))
                .color(p.muted),
            );
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Have another DeepCool cooler?").color(p.muted));
            if ui.button("Request support for your cooler").clicked() {
                let url = crate::feedback::device_url(app);
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        });
    });
    ui.add_space(24.0);
}

/// The cooler's product picture in the chosen finish, with the live preview
/// on the pump's screen when it's the connected one.
fn picture(app: &mut App, ui: &mut egui::Ui, width: f32, connected: bool) {
    let pic = picture_of(app.prefs.model, app.prefs.finish);
    let size = vec2(width, width / pic.aspect);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    egui::Image::new(pic.source)
        .fit_to_exact_size(size)
        .paint_at(ui, rect);
    if connected {
        let at = |x: f32, y: f32| {
            pos2(
                rect.left() + x * rect.width(),
                rect.top() + y * rect.height(),
            )
        };
        let screen = Rect::from_min_max(
            at(pic.screen.min.x, pic.screen.min.y),
            at(pic.screen.max.x, pic.screen.max.y),
        );
        // Whole physical pixels, resampled to exactly that size.
        let ppp = ui.ctx().pixels_per_point();
        let pixels = [
            (screen.width() * ppp).round() as usize,
            (screen.height() * ppp).round() as usize,
        ];
        let screen = Rect::from_min_size(
            pos2(
                (screen.min.x * ppp).round() / ppp,
                (screen.min.y * ppp).round() / ppp,
            ),
            vec2(pixels[0] as f32, pixels[1] as f32) / ppp,
        );
        let ctx = ui.ctx().clone();
        let Some(texture) = app.preview.small(&ctx, pixels) else {
            return;
        };
        ui.painter().image(
            texture,
            screen,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}
