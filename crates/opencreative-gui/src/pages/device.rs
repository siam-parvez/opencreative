//! The cooler page: what its screen shows, laid out like the vendor app's
//! device page (display controls on the left, preview and content on the
//! right). Every change applies immediately.

use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use opencreative_config::{FitConfig, Metric, Rgb, SceneConfig};

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets::{section, segmented, toggle};
use crate::{App, Page, is_digital};

const CONTROLS_WIDTH: f32 = 340.0;
/// Upper bound on how much the 320x240 preview is enlarged.
const MAX_PREVIEW_SCALE: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Digital,
    Multimedia,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Style {
    Panels,
    Gauges,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    Image,
    Color,
}

/// One-click layouts, like the vendor app's "recommended combinations".
const PRESETS: [(&str, Metric, [Metric; 2]); 3] = [
    (
        "CPU monitoring",
        Metric::CpuTemp,
        [Metric::CpuFreq, Metric::CpuPower],
    ),
    (
        "CPU and GPU",
        Metric::CpuTemp,
        [Metric::GpuTemp, Metric::GpuLoad],
    ),
    (
        "System load",
        Metric::CpuLoad,
        [Metric::GpuLoad, Metric::RamUsage],
    ),
];

const SWATCHES: [[u8; 3]; 8] = [
    [0x0d, 0x9b, 0x94],
    [0x22, 0xd3, 0xee],
    [0x3b, 0x82, 0xf6],
    [0x8b, 0x5c, 0xf6],
    [0xec, 0x48, 0x99],
    [0xef, 0x44, 0x44],
    [0xf5, 0x9e, 0x0b],
    [0xff, 0xff, 0xff],
];

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let mut mode = if is_digital(&app.applied.scene) {
        Mode::Digital
    } else {
        Mode::Multimedia
    };

    ui.horizontal(|ui| {
        let crumb = ui
            .add(
                egui::Label::new(
                    egui::RichText::new("Devices")
                        .font(theme::semibold(24.0))
                        .color(p.muted),
                )
                .sense(Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if crumb.clicked() {
            app.page = Page::Devices;
        }
        ui.label(
            egui::RichText::new("/")
                .font(theme::semibold(24.0))
                .color(p.faint),
        );
        let name = app
            .device
            .clone()
            .unwrap_or_else(|| opencreative_device::SUPPORTED[0].name.to_owned());
        ui.label(
            egui::RichText::new(name)
                .font(theme::semibold(24.0))
                .color(p.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if segmented(
                ui,
                &mut mode,
                &[(Mode::Digital, "Digital"), (Mode::Multimedia, "Multimedia")],
            ) {
                let scene = match mode {
                    Mode::Digital => app.digital_scene.clone(),
                    Mode::Multimedia => app.media_scene.clone(),
                };
                app.set_scene(scene);
            }
            ui.label(egui::RichText::new("Mode").color(p.muted));
        });
    });
    ui.add_space(14.0);

    if app.device.is_none() {
        notice(app, ui);
        ui.add_space(12.0);
    }

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(CONTROLS_WIDTH);
            theme::card(ui, |ui| display_controls(app, ui, mode));
        });
        ui.add_space(4.0);
        ui.vertical(|ui| {
            theme::card(ui, |ui| {
                preview(app, ui);
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                match mode {
                    Mode::Digital => digital(app, ui),
                    Mode::Multimedia => multimedia(app, ui),
                }
            });
        });
    });
    ui.add_space(56.0);
}

fn notice(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    egui::Frame::new()
        .fill(p.warn.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.5)))
        .corner_radius(10)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::show(ui, Icon::Cooler, 20.0, p.warn);
                ui.label(
                    egui::RichText::new(
                        "No cooler connected. Your changes are saved and shown as soon as one is.",
                    )
                    .color(p.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Check USB access").clicked() {
                        app.page = Page::Settings;
                    }
                });
            });
        });
}

fn display_controls(app: &mut App, ui: &mut egui::Ui, mode: Mode) {
    let p = Palette::of(ui.ctx());
    section(ui, "Display");
    ui.add_space(8.0);

    labelled_value(ui, "Brightness", &format!("{}%", app.brightness));
    ui.spacing_mut().slider_width = ui.available_width();
    let slider = ui.add(egui::Slider::new(&mut app.brightness, 0..=100).show_value(false));
    // Send once the user lets go, not on every step of a drag.
    if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
        app.set_brightness(app.brightness);
    }
    ui.add_space(14.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Zen mode").color(p.text));
        icons::show(ui, Icon::Help, 15.0, p.faint).on_hover_text(
            "Turns the screen off. Your layout and brightness are kept for when you turn it back on.",
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut zen = !app.screen_on;
            if toggle(ui, &mut zen).changed() {
                app.set_screen(!zen);
            }
        });
    });
    ui.add_space(14.0);

    ui.label(egui::RichText::new("Rotation").color(p.text));
    let mut rotation = app.applied.rotation;
    if segmented(
        ui,
        &mut rotation,
        &[(0, "0°"), (90, "90°"), (180, "180°"), (270, "270°")],
    ) {
        app.set_rotation(rotation);
    }

    if mode == Mode::Digital
        && let Some(mut secs) = interval(&app.applied.scene)
    {
        ui.add_space(14.0);
        labelled_value(ui, "Refresh every", &format!("{secs:.1} s"));
        let slider = ui.add(egui::Slider::new(&mut secs, 0.5..=5.0).show_value(false));
        if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
            let mut scene = app.applied.scene.clone();
            set_interval(&mut scene, secs);
            app.set_scene(scene);
        }
    }
}

/// "Label ........ value" on one line.
fn labelled_value(ui: &mut egui::Ui, label: &str, value: &str) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).color(p.muted));
        });
    });
}

fn preview(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Preview")
                .font(theme::semibold(15.0))
                .color(p.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, Icon::Rotate, "Rotate 90°").clicked() {
                let next = (app.applied.rotation + 90) % 360;
                app.set_rotation(next);
            }
        });
    });
    ui.add_space(6.0);

    let (w, h) = app.preview.size;
    let pad = 14.0;
    let fit = (ui.available_width() - 2.0 * pad) / w.max(1) as f32;
    let scale = fit.min(MAX_PREVIEW_SCALE);
    let screen = vec2(w as f32, h as f32) * scale;
    ui.vertical_centered(|ui| {
        let (bezel, _) =
            ui.allocate_exact_size(screen + vec2(2.0 * pad, 2.0 * pad), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(bezel, 16.0, p.bezel);
        let inner = bezel.shrink(pad);
        match &app.preview.texture {
            Some(texture) => {
                painter.image(
                    texture.id(),
                    inner,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.rect_filled(inner, 2.0, Color32::BLACK);
            }
        }
        if !app.screen_on {
            painter.rect_filled(inner, 2.0, Color32::from_black_alpha(210));
            painter.text(
                inner.center(),
                egui::Align2::CENTER_CENTER,
                "Zen mode — screen off",
                theme::medium(15.0),
                Color32::from_gray(170),
            );
        }
    });
    if let Some(e) = &app.preview.error {
        ui.colored_label(p.bad, e);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        toggle(ui, &mut app.preview.accurate);
        ui.label(egui::RichText::new("Show the panel's 16-bit colors").color(p.muted))
            .on_hover_text("Preview the exact colors the LCD can display (RGB565 with dithering)");
    });
}

fn digital(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let scene = app.applied.scene.clone();
    let secs = interval(&scene).unwrap_or(1.0);

    section(ui, "Style");
    let mut style = if matches!(scene, SceneConfig::Monitor { .. }) {
        Style::Gauges
    } else {
        Style::Panels
    };
    if segmented(
        ui,
        &mut style,
        &[(Style::Panels, "Panels"), (Style::Gauges, "Gauges")],
    ) {
        let next = match style {
            Style::Gauges => SceneConfig::Monitor {
                interval_secs: secs,
            },
            Style::Panels => {
                let mut panels = SceneConfig::default();
                set_interval(&mut panels, secs);
                panels
            }
        };
        app.set_scene(next);
        return;
    }
    ui.add_space(12.0);

    match scene {
        SceneConfig::Panels {
            mut primary,
            mut secondary,
            interval_secs,
        } => {
            section(ui, "Display data");
            let mut changed = false;
            egui::Grid::new("display-data")
                .num_columns(2)
                .spacing(vec2(16.0, 8.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Main reading").color(p.muted));
                    changed |= metric_combo(ui, "primary", &mut primary);
                    ui.end_row();
                    ui.label(egui::RichText::new("Second reading").color(p.muted));
                    changed |= metric_combo(ui, "secondary-1", &mut secondary[0]);
                    ui.end_row();
                    ui.label(egui::RichText::new("Third reading").color(p.muted));
                    changed |= metric_combo(ui, "secondary-2", &mut secondary[1]);
                    ui.end_row();
                });
            ui.add_space(12.0);

            section(ui, "Recommended");
            for (name, main, others) in PRESETS {
                let selected = primary == main && secondary == others;
                let detail = format!(
                    "{} (main) + {} + {}",
                    main.name(),
                    others[0].name(),
                    others[1].name()
                );
                let response = ui.radio(selected, egui::RichText::new(name).color(p.text));
                ui.indent(name, |ui| {
                    ui.label(egui::RichText::new(detail).small().color(p.muted));
                });
                if response.clicked() && !selected {
                    primary = main;
                    secondary = others;
                    changed = true;
                }
            }
            if changed {
                app.set_scene(SceneConfig::Panels {
                    primary,
                    secondary,
                    interval_secs,
                });
            }
        }
        _ => {
            ui.label(
                egui::RichText::new(
                    "Gauges show CPU temperature and load, with GPU temperature, RAM and the clock below.",
                )
                .color(p.muted),
            );
        }
    }
}

fn metric_combo(ui: &mut egui::Ui, id: &str, metric: &mut Metric) -> bool {
    let before = *metric;
    egui::ComboBox::from_id_salt(id)
        .width(220.0)
        .selected_text(metric.name())
        .show_ui(ui, |ui| {
            for m in Metric::ALL {
                ui.selectable_value(metric, m, m.name());
            }
        });
    *metric != before
}

fn multimedia(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let scene = app.applied.scene.clone();
    let mut source = if matches!(scene, SceneConfig::Solid { .. }) {
        Source::Color
    } else {
        Source::Image
    };

    section(ui, "Source");
    if segmented(
        ui,
        &mut source,
        &[(Source::Image, "Image or GIF"), (Source::Color, "Color")],
    ) {
        match source {
            Source::Color => app.set_scene(SceneConfig::Solid {
                color: Rgb(SWATCHES[0]),
            }),
            Source::Image => match &app.media_scene {
                image @ SceneConfig::Image { .. } => app.set_scene(image.clone()),
                _ => app.browse_for_image(&ctx),
            },
        }
        return;
    }
    ui.add_space(10.0);

    match scene {
        SceneConfig::Image { path, mut fit } => {
            drop_zone(app, ui);
            ui.add_space(10.0);
            let file = path.file_name().map_or_else(
                || path.display().to_string(),
                |f| f.to_string_lossy().into(),
            );
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Showing").color(p.muted));
                ui.label(egui::RichText::new(file).color(p.text))
                    .on_hover_text(path.display().to_string());
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Fit").color(p.muted));
            if segmented(
                ui,
                &mut fit,
                &[
                    (FitConfig::Cover, "Fill"),
                    (FitConfig::Contain, "Fit"),
                    (FitConfig::Stretch, "Stretch"),
                ],
            ) {
                app.set_scene(SceneConfig::Image { path, fit });
            }
        }
        SceneConfig::Solid { color } => {
            let mut rgb = color.0;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Color").color(p.muted));
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    app.set_scene(SceneConfig::Solid { color: Rgb(rgb) });
                }
                ui.add_space(8.0);
                for swatch in SWATCHES {
                    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    let fill = Color32::from_rgb(swatch[0], swatch[1], swatch[2]);
                    ui.painter().circle_filled(rect.center(), 10.0, fill);
                    let ring = if swatch == rgb { p.accent } else { p.border };
                    ui.painter()
                        .circle_stroke(rect.center(), 11.5, Stroke::new(1.5, ring));
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        app.set_scene(SceneConfig::Solid { color: Rgb(swatch) });
                    }
                }
            });
        }
        _ => {
            drop_zone(app, ui);
        }
    }
}

fn drop_zone(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    egui::Frame::new()
        .fill(p.accent_soft)
        .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.6)))
        .corner_radius(10)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                icons::show(ui, Icon::Storage, 26.0, p.accent);
                ui.label(
                    egui::RichText::new("Drop an image or GIF anywhere on this window")
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new("PNG, JPEG, WebP, BMP or animated GIF")
                        .small()
                        .color(p.muted),
                );
                ui.add_space(4.0);
                let browsing = app.picking_file.is_some();
                if ui
                    .add_enabled(!browsing, egui::Button::new("Browse…"))
                    .clicked()
                {
                    app.browse_for_image(&ctx);
                }
            });
        });
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.accent_soft);
    }
    icons::paint(ui.painter(), rect.shrink(6.0), icon, p.muted);
    response
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn interval(scene: &SceneConfig) -> Option<f64> {
    match scene {
        SceneConfig::Panels { interval_secs, .. } | SceneConfig::Monitor { interval_secs } => {
            Some(*interval_secs)
        }
        _ => None,
    }
}

fn set_interval(scene: &mut SceneConfig, secs: f64) {
    if let SceneConfig::Panels { interval_secs, .. } | SceneConfig::Monitor { interval_secs } =
        scene
    {
        *interval_secs = secs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_is_read_and_written_for_live_scenes_only() {
        let mut scene = SceneConfig::default();
        set_interval(&mut scene, 2.5);
        assert_eq!(interval(&scene), Some(2.5));
        let mut solid = SceneConfig::Solid {
            color: Rgb([0, 0, 0]),
        };
        set_interval(&mut solid, 2.5);
        assert_eq!(interval(&solid), None);
    }

    #[test]
    fn presets_are_distinct() {
        for (i, a) in PRESETS.iter().enumerate() {
            for b in &PRESETS[i + 1..] {
                assert!((a.1, a.2) != (b.1, b.2));
            }
        }
    }
}
