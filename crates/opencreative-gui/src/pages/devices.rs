//! Devices: supported coolers found on USB.

use eframe::egui::{self, Color32, Sense, vec2};

use crate::icons::{self, Icon};
use crate::pages::header;
use crate::theme::{self, Palette};
use crate::widgets::chip;
use crate::{App, Page};

const THUMB: egui::Vec2 = vec2(176.0, 132.0);

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    header(
        ui,
        "Devices",
        Some("DeepCool coolers connected to this computer"),
    );

    if app.devices.is_empty() {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                icons::show(ui, Icon::Cooler, 48.0, p.faint);
                ui.add_space(8.0);
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
                ui.add_space(8.0);
                if ui.button("Open Settings").clicked() {
                    app.page = Page::Settings;
                }
                ui.add_space(18.0);
            });
        });
    }

    let rows: Vec<_> = app
        .devices
        .iter()
        .map(|d| (d.name, d.width, d.height, d.serial.clone()))
        .collect();
    for (name, width, height, serial) in rows {
        let connected = app.device.as_deref() == Some(name);
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                thumbnail(app, ui, connected);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(name)
                            .font(theme::semibold(18.0))
                            .color(p.text),
                    );
                    ui.label(
                        egui::RichText::new(format!("{width} × {height} LCD · USB")).color(p.muted),
                    );
                    if let Some(serial) = &serial {
                        ui.label(
                            egui::RichText::new(format!("Serial {serial}"))
                                .small()
                                .color(p.faint),
                        );
                    }
                    ui.add_space(6.0);
                    let (text, color) = if connected {
                        ("Connected", p.good)
                    } else {
                        ("Found, not in use", p.faint)
                    };
                    chip(ui, text, color);
                    ui.add_space(8.0);
                    if ui.button("Customize screen").clicked() {
                        app.page = Page::Device;
                    }
                });
            });
        });
        ui.add_space(12.0);
    }

    ui.add_space(4.0);
    theme::card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new("Supported models")
                .font(theme::semibold(15.0))
                .color(p.text),
        );
        for spec in opencreative_device::SUPPORTED {
            ui.label(
                egui::RichText::new(format!(
                    "{}  ·  USB {:04x}:{:04x}  ·  {} × {}",
                    spec.name, spec.vendor_id, spec.product_id, spec.width, spec.height
                ))
                .color(p.muted),
            );
        }
        ui.label(
            egui::RichText::new(
                "Another DeepCool cooler? Open an issue with its USB ID to help add support.",
            )
            .small()
            .color(p.faint),
        );
    });
    ui.add_space(56.0);
}

/// The cooler as a framed screen, showing the live preview when connected.
fn thumbnail(app: &App, ui: &mut egui::Ui, connected: bool) {
    let p = Palette::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(THUMB, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, p.bezel);
    let inner = rect.shrink(10.0);
    match (&app.preview.texture, connected) {
        (Some(texture), true) => {
            painter.image(
                texture.id(),
                inner,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        _ => {
            painter.rect_filled(inner, 2.0, Color32::from_gray(12));
        }
    }
}
