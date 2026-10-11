//! Settings: background service, USB access, app menu, appearance, about.

use eframe::egui;

use crate::pages::{header, setting_row};
use crate::prefs::Appearance;
use crate::theme::{self, Palette};
use crate::widgets::{chip, segmented, toggle};
use crate::{App, SERVICE_SUPPORTED, service};

const REPOSITORY: &str = "https://github.com/siam-parvez/opencreative";

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    header(ui, "Settings", None);
    let ctx = ui.ctx().clone();
    let p = Palette::of(&ctx);

    group(ui, "Startup", |ui| {
        if !SERVICE_SUPPORTED {
            ui.label(
                egui::RichText::new("Running in the background is Linux-only for now.")
                    .color(p.muted),
            );
            return;
        }
        let state = app.service.unwrap_or_else(service::query);
        let status = match (state.enabled, state.active) {
            (_, true) => "Running now.",
            (true, false) => "Enabled, but not running.",
            (false, false) => "Off: the screen updates only while this window is open.",
        };
        setting_row(
            ui,
            "Start with the computer",
            &format!(
                "Runs in the background from login and keeps the cooler's screen updated; \
                 open this window from the app menu. {status}"
            ),
            |ui| {
                let mut on = state.enabled;
                if toggle(ui, &mut on).changed() {
                    if on {
                        app.enable_service();
                    } else {
                        app.disable_service();
                    }
                }
            },
        );
    });

    group(ui, "USB access", |ui| {
        if service::udev_rule_installed() {
            setting_row(
                ui,
                "USB rule installed",
                "OpenCreative can use the cooler without root.",
                |ui| {
                    chip(ui, "Installed", p.good);
                },
            );
            return;
        }
        let busy = app.task.is_some();
        setting_row(
            ui,
            "Allow access to the cooler",
            "Installs a udev rule so OpenCreative can use the cooler without root. Asks for your password.",
            |ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("Install"))
                    .clicked()
                {
                    app.run_task(&ctx, "Installing the USB rule", service::install_udev_rule);
                }
            },
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Or run this in a terminal:")
                .small()
                .color(p.muted),
        );
        let command = service::udev_install_command();
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(egui::RichText::new(&command).code()).truncate());
            if ui.small_button("Copy").clicked() {
                ctx.copy_text(command.clone());
                app.notify("Command copied.");
            }
        });
    });

    if SERVICE_SUPPORTED {
        group(ui, "App menu", |ui| {
            setting_row(
                ui,
                "Show in the app menu",
                "Adds OpenCreative with its icon to your desktop's launcher.",
                |ui| {
                    let mut on = service::desktop_entry_installed();
                    if toggle(ui, &mut on).changed() {
                        let result = if on {
                            service::install_desktop_entry()
                        } else {
                            service::remove_desktop_entry()
                        };
                        match result {
                            Ok(()) if on => app.notify("Added to the app menu."),
                            Ok(()) => app.notify("Removed from the app menu."),
                            Err(e) => app.fail(e),
                        }
                    }
                },
            );
        });
    }

    group(ui, "Integrations", |ui| {
        let detected = app.history.latest().is_some_and(|s| s.claude.is_some());
        let status = if detected {
            "OpenCUT detected."
        } else {
            "OpenCUT isn't running (start its ocutd service)."
        };
        setting_row(
            ui,
            "Claude usage",
            &format!(
                "Shows your Claude plan usage (5-hour and weekly) from OpenCUT on the \
                 dashboard and as readings for the cooler screen. {status}"
            ),
            |ui| {
                let mut on = app.prefs.claude_usage;
                if toggle(ui, &mut on).changed() {
                    app.prefs.claude_usage = on;
                    app.save_prefs();
                }
            },
        );
    });

    group(ui, "Appearance", |ui| {
        setting_row(ui, "Theme", "Light, dark, or follow the desktop.", |ui| {
            let mut appearance = app.prefs.appearance;
            if segmented(
                ui,
                &mut appearance,
                &[
                    (Appearance::System, "System"),
                    (Appearance::Light, "Light"),
                    (Appearance::Dark, "Dark"),
                ],
            ) {
                app.prefs.appearance = appearance;
                ctx.set_theme(appearance.preference());
                app.save_prefs();
            }
        });
    });

    group(ui, "About", |ui| {
        ui.label(
            egui::RichText::new(format!("OpenCreative {}", env!("CARGO_PKG_VERSION")))
                .font(theme::semibold(16.0))
                .color(p.text),
        );
        ui.label(
            egui::RichText::new("Open-source control for DeepCool LCD coolers.").color(p.muted),
        );
        ui.hyperlink_to("Source code and issues", REPOSITORY);
        ui.label(
            egui::RichText::new("MIT or Apache-2.0. Not affiliated with or endorsed by DeepCool.")
                .small()
                .color(p.faint),
        );
    });
    ui.add_space(56.0);
}

/// A titled card spanning the page width.
fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let p = Palette::of(ui.ctx());
    theme::card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(title)
                .font(theme::semibold(13.0))
                .color(p.faint),
        );
        ui.add_space(6.0);
        add(ui);
    });
    ui.add_space(12.0);
}
