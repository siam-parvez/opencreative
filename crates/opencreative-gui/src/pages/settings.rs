//! Settings: background service, USB access, app menu, appearance, about.

use eframe::egui;
use opencreative_sensors::ClaudeStatus;

use crate::pages::{header, setting_row};
use crate::prefs::Appearance;
use crate::theme::{self, Palette};
use crate::widgets::{card_look, chip, paint_mini_window, preview_tiles, toggle};
use crate::{App, SERVICE_SUPPORTED, service};

const REPOSITORY: &str = "https://github.com/siam-parvez/opencreative";

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    header(ui, "Settings", None);
    let ctx = ui.ctx().clone();
    let p = Palette::of(&ctx);

    group(ui, "Appearance", |ui| {
        ui.label(
            egui::RichText::new("Theme")
                .font(theme::medium(15.0))
                .color(p.text),
        );
        ui.label(egui::RichText::new("Light, dark, or follow the desktop.").color(p.muted));
        ui.add_space(8.0);
        let mut appearance = app.prefs.appearance;
        if preview_tiles(
            ui,
            &mut appearance,
            &[
                (Appearance::System, "System"),
                (Appearance::Light, "Light"),
                (Appearance::Dark, "Dark"),
            ],
            egui::vec2(146.0, 112.0),
            |painter, rect, option| match option {
                Appearance::Light => paint_mini_window(painter, rect, &theme::LIGHT),
                Appearance::Dark => paint_mini_window(painter, rect, &theme::DARK),
                // Light on the left, dark on the right.
                Appearance::System => {
                    let half = rect.center().x;
                    let left = egui::Rect::from_min_max(rect.min, egui::pos2(half, rect.max.y));
                    let right = egui::Rect::from_min_max(egui::pos2(half, rect.min.y), rect.max);
                    paint_mini_window(&painter.with_clip_rect(left), rect, &theme::LIGHT);
                    paint_mini_window(&painter.with_clip_rect(right), rect, &theme::DARK);
                }
            },
        ) {
            app.prefs.appearance = appearance;
            ctx.set_theme(appearance.preference());
            app.save_prefs();
        }
        ui.add_space(16.0);
        font_picker(app, ui);
    });

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
        // The icon belongs to the background service.
        ui.add_space(12.0);
        let external = matches!(app.link, crate::Link::External);
        let hint = if external {
            ""
        } else {
            " Available once the background service is running."
        };
        setting_row(
            ui,
            "Show in the top bar",
            &format!(
                "An OpenCreative icon next to the clock, to open this window, or to \
                 switch modes and looks, brightness, colors and rotation without it.{hint}"
            ),
            |ui| {
                let mut on = !app.applied.hide_tray;
                let changed = ui.add_enabled_ui(external, |ui| toggle(ui, &mut on)).inner;
                if changed.changed() && app.send(opencreative_ipc::Request::SetTray { on }) {
                    app.applied.hide_tray = !on;
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
        group(ui, "Power readings", |ui| {
            let installed = service::power_rule_installed();
            let readable = app.history.power_readable();
            let status = match (installed, readable) {
                (true, true) => " On.",
                (true, false) => {
                    " The rule is installed but this computer has no readable counters."
                }
                (false, _) => "",
            };
            setting_row(
                ui,
                "CPU and PC power",
                &format!(
                    "Lets OpenCreative read the CPU's energy counters, for CPU power and an \
                     estimate of the whole PC's power. Any program on this computer can then \
                     read them too, which is a known side channel (CVE-2020-8694), so leave \
                     this off on shared computers. Asks for your password.{status}"
                ),
                |ui| {
                    let mut on = installed;
                    let busy = app.task.is_some();
                    let changed = ui.add_enabled_ui(!busy, |ui| toggle(ui, &mut on)).inner;
                    if changed.changed() {
                        if on {
                            app.run_task(&ctx, "Allowing power readings", || {
                                service::set_power_rule(true)
                            });
                        } else {
                            app.run_task(&ctx, "Turning off power readings", || {
                                service::set_power_rule(false)
                            });
                        }
                    }
                },
            );
        });

        group(ui, "App menu", |ui| {
            setting_row(
                ui,
                "Show in the app menu",
                if service::packaged() {
                    "Your package already adds OpenCreative to the app menu."
                } else {
                    "Adds OpenCreative with its icon to your desktop's launcher."
                },
                |ui| {
                    if service::packaged() {
                        return;
                    }
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
        let status = match opencreative_sensors::claude_status() {
            ClaudeStatus::Off => "",
            ClaudeStatus::Ok => " Connected.",
            ClaudeStatus::NotSignedIn => " Sign in to Claude Code on this computer first.",
            ClaudeStatus::Expired => {
                " Your Claude Code login expired; run `claude` once to renew it."
            }
            ClaudeStatus::Unreachable => " Can't reach Anthropic right now; retrying.",
        };
        setting_row(
            ui,
            "Claude usage",
            &format!(
                "Shows your 5-hour and weekly Claude limits on the dashboard and the cooler \
                 screen, using the login Claude Code keeps on this computer. Checked every \
                 90 seconds; your login is only sent to Anthropic.{status}"
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

    if SERVICE_SUPPORTED {
        group(ui, "Uninstall", |ui| {
            let description = if service::packaged() {
                "Installed by your package manager: remove OpenCreative from your software center."
            } else {
                "Removes the program, the background service, the app-menu entry and the USB rule."
            };
            setting_row(ui, "Uninstall OpenCreative", description, |ui| {
                if !service::packaged() && ui.button("Uninstall…").clicked() {
                    app.uninstall_prompt = Some(false);
                }
            });
        });
    }

    group(ui, "Help & feedback", |ui| {
        ui.label(
            egui::RichText::new(
                "Opens a form on GitHub (a free account is needed) with your OpenCreative \
                 version, Linux version and cooler already filled in. You see everything \
                 before it's sent.",
            )
            .color(p.muted),
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Report a problem").clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(crate::feedback::problem_url(app)));
            }
            if ui.button("Request support for your cooler").clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(crate::feedback::device_url(app)));
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

/// The app font: a card per choice, each showing itself.
fn font_picker(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    ui.label(
        egui::RichText::new("App font")
            .font(theme::medium(15.0))
            .color(p.text),
    );
    ui.label(
        egui::RichText::new("The typeface of this window. The cooler screen keeps its own.")
            .color(p.muted),
    );
    ui.add_space(8.0);
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for font in theme::AppFont::ALL {
            let selected = app.prefs.app_font == font;
            let response = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .id_salt(("app-font", font.name()))
                        .sense(egui::Sense::click()),
                    |ui| {
                        let (fill, stroke) = card_look(
                            ui,
                            ui.id(),
                            selected,
                            ui.response().hovered(),
                            p.raised,
                            p.card,
                        );
                        egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(theme::SMALL_RADIUS)
                            .inner_margin(egui::Margin::symmetric(14, 10))
                            .show(ui, |ui| {
                                ui.set_width(118.0);
                                ui.set_height(56.0);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.label(
                                        egui::RichText::new("Aa")
                                            .font(font.sample(26.0))
                                            .color(if selected { p.accent } else { p.text }),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(font.name())
                                                .font(font.sample(13.0))
                                                .color(p.muted),
                                        )
                                        .truncate(),
                                    );
                                });
                            });
                    },
                )
                .response
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() && !selected {
                picked = Some(font);
            }
        }
    });
    if let Some(font) = picked {
        app.prefs.app_font = font;
        theme::set_app_font(ui.ctx(), font);
        app.save_prefs();
    }
}
