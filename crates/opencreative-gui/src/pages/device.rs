//! The cooler page: what its screen shows, laid out like the vendor app's
//! device page (display controls on the left, preview and content on the
//! right). Every change applies immediately.

use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use std::time::{Duration, Instant};

use opencreative_config::{FitConfig, Metric, Rgb, SceneConfig, SkinConfig};
use opencreative_render::panels::Layout;

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets::{section, segmented, toggle};
use crate::{App, Page, is_digital};

const CONTROLS_WIDTH: f32 = 300.0;
/// Upper bound on how much the 320x240 preview is enlarged.
const MAX_PREVIEW_ZOOM: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Digital,
    Multimedia,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    Media,
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

/// Offered when the Claude integration is on.
const CLAUDE_PRESET: (&str, Metric, [Metric; 2]) = (
    "Claude usage",
    Metric::ClaudeSession,
    [Metric::ClaudeWeekly, Metric::CpuTemp],
);

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
                        .font(theme::semibold(22.0))
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
                .font(theme::semibold(22.0))
                .color(p.faint),
        );
        let name = app
            .device
            .clone()
            .unwrap_or_else(|| opencreative_device::SUPPORTED[0].name.to_owned());
        let name = app.device_title(&name).to_owned();
        ui.label(
            egui::RichText::new(name)
                .font(theme::semibold(22.0))
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
            let mut zen = app.zen;
            if toggle(ui, &mut zen).changed() {
                app.set_zen(zen);
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
    let pad = 10.0;
    // Whole-number zoom in physical pixels keeps every panel pixel sharp.
    let ppp = ui.ctx().pixels_per_point();
    let room = (ui.available_width() - 2.0 * pad) * ppp;
    let zoom = (room / w.max(1) as f32)
        .floor()
        .clamp(1.0, MAX_PREVIEW_ZOOM);
    let screen = vec2(w as f32, h as f32) * zoom / ppp;
    ui.vertical_centered(|ui| {
        let (bezel, response) =
            ui.allocate_exact_size(screen + vec2(2.0 * pad, 2.0 * pad), Sense::drag());
        pan_and_zoom(app, ui, &response, screen);
        let painter = ui.painter();
        painter.rect_filled(bezel, 16.0, p.bezel);
        let inner = snap_to_pixels(bezel.shrink(pad), ppp, screen);
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
    });
    if let Some(e) = &app.preview.error {
        ui.colored_label(p.bad, e);
    }
}

/// Dragging the preview moves the selected playlist item's focus point;
/// scrolling zooms. Both edit a draft that's sent when the edit pauses.
fn pan_and_zoom(app: &mut App, ui: &egui::Ui, response: &egui::Response, screen: egui::Vec2) {
    if is_digital(&app.applied.scene) || matches!(app.applied.scene, SceneConfig::Solid { .. }) {
        return;
    }
    let (mut items, secs) = app.playlist();
    let Some(item) = items.get_mut(app.selected_item) else {
        return;
    };
    let mut edited = false;
    if response.dragged() {
        let delta = response.drag_delta();
        // Moving the picture right shows more of its left: the focus moves
        // the other way, less so the more it's zoomed in.
        item.focus[0] = (item.focus[0] - delta.x / screen.x / item.zoom).clamp(0.0, 1.0);
        item.focus[1] = (item.focus[1] - delta.y / screen.y / item.zoom).clamp(0.0, 1.0);
        edited = true;
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            item.zoom = (item.zoom * (1.0 + scroll * 0.003))
                .clamp(1.0, opencreative_render::media::Crop::MAX_ZOOM);
            edited = true;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if edited {
        app.set_playlist(items, secs, true);
    }
}

/// `rect` moved to start on a physical pixel, with exactly `size`.
fn snap_to_pixels(rect: egui::Rect, ppp: f32, size: egui::Vec2) -> egui::Rect {
    let min = egui::pos2(
        (rect.min.x * ppp).round() / ppp,
        (rect.min.y * ppp).round() / ppp,
    );
    egui::Rect::from_min_size(min, size)
}

fn digital(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let scene = app.applied.scene.clone();
    let secs = interval(&scene).unwrap_or(1.0);

    section(ui, "Skin");
    if let Some(next) = skin_gallery(app, ui, &scene, secs) {
        app.set_scene(next);
        return;
    }
    ui.add_space(12.0);

    match scene {
        SceneConfig::Panels {
            skin,
            mut primary,
            mut secondary,
            interval_secs,
        } => {
            let claude = app.prefs.claude_usage;
            section(ui, "Display data");
            let mut changed = false;
            egui::Grid::new("display-data")
                .num_columns(2)
                .spacing(vec2(16.0, 8.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Main reading").color(p.muted));
                    changed |= metric_combo(ui, "primary", &mut primary, claude);
                    ui.end_row();
                    ui.label(egui::RichText::new("Second reading").color(p.muted));
                    changed |= metric_combo(ui, "secondary-1", &mut secondary[0], claude);
                    ui.end_row();
                    ui.label(egui::RichText::new("Third reading").color(p.muted));
                    // The HUD skin charts network traffic in that spot instead.
                    let shown = skin != SkinConfig::Hud;
                    ui.add_enabled_ui(shown, |ui| {
                        changed |= metric_combo(ui, "secondary-2", &mut secondary[1], claude);
                    })
                    .response
                    .on_disabled_hover_text("The HUD skin shows network speeds here.");
                    ui.end_row();
                });
            ui.add_space(12.0);

            section(ui, "Recommended");
            let presets = PRESETS.into_iter().chain(claude.then_some(CLAUDE_PRESET));
            for (name, main, others) in presets {
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
                    skin,
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

/// A choice in the skin gallery: a Panels skin, or the gauges screen.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Look {
    Skin(SkinConfig),
    Gauges,
}

impl Look {
    const ALL: [Look; 3] = [
        Look::Skin(SkinConfig::Lines),
        Look::Skin(SkinConfig::Hud),
        Look::Gauges,
    ];

    fn name(self) -> &'static str {
        match self {
            Look::Skin(skin) => skin.name(),
            Look::Gauges => "Gauges",
        }
    }
}

/// Live thumbnails for the skin gallery, re-rendered every few seconds.
#[derive(Default)]
pub struct SkinThumbs {
    rendered: Option<Instant>,
    textures: Vec<egui::TextureHandle>,
}

const THUMB_REFRESH: Duration = Duration::from_secs(2);
const THUMB_SIZE: egui::Vec2 = vec2(176.0, 132.0);

/// Shows each look with the user's readings; returns the scene to switch to
/// when one is clicked.
fn skin_gallery(
    app: &mut App,
    ui: &mut egui::Ui,
    scene: &SceneConfig,
    secs: f64,
) -> Option<SceneConfig> {
    let p = Palette::of(ui.ctx());
    let (current, layout) = match scene {
        SceneConfig::Panels {
            skin,
            primary,
            secondary,
            ..
        } => (
            Look::Skin(*skin),
            Layout {
                primary: *primary,
                secondary: *secondary,
            },
        ),
        _ => (Look::Gauges, default_layout()),
    };
    refresh_thumbs(app, ui.ctx(), &layout);

    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (look, texture) in Look::ALL.into_iter().zip(&app.skin_thumbs.textures) {
            let selected = look == current;
            let stroke = if selected {
                Stroke::new(2.0, p.accent)
            } else {
                Stroke::new(1.0, p.border)
            };
            let response = egui::Frame::new()
                .stroke(stroke)
                .corner_radius(10)
                .inner_margin(8)
                .show(ui, |ui| {
                    // Keep each card as narrow as its thumbnail.
                    ui.set_max_width(THUMB_SIZE.x);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Image::new((texture.id(), THUMB_SIZE)).corner_radius(6));
                        ui.label(egui::RichText::new(look.name()).color(if selected {
                            p.accent
                        } else {
                            p.text
                        }));
                    });
                })
                .response
                .interact(Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() && !selected {
                picked = Some(look);
            }
        }
    });
    picked.map(|look| match look {
        Look::Gauges => SceneConfig::Monitor {
            interval_secs: secs,
        },
        Look::Skin(skin) => SceneConfig::Panels {
            skin,
            primary: layout.primary,
            secondary: layout.secondary,
            interval_secs: secs,
        },
    })
}

fn default_layout() -> Layout {
    match SceneConfig::default() {
        SceneConfig::Panels {
            primary, secondary, ..
        } => Layout { primary, secondary },
        _ => unreachable!("the default scene is Panels"),
    }
}

fn refresh_thumbs(app: &mut App, ctx: &egui::Context, layout: &Layout) {
    let thumbs = &mut app.skin_thumbs;
    let fresh = thumbs
        .rendered
        .is_some_and(|at| at.elapsed() < THUMB_REFRESH);
    if fresh && thumbs.textures.len() == Look::ALL.len() {
        return;
    }
    let snap = app.history.latest().cloned().unwrap_or_default();
    let history = app.history.samples();
    let clock = chrono::Local::now().format("%H:%M").to_string();
    // Render at the thumbnail's exact pixel size so nothing is resampled.
    let ppp = ctx.pixels_per_point();
    let (w, h) = (
        (THUMB_SIZE.x * ppp).round() as u32,
        (THUMB_SIZE.y * ppp).round() as u32,
    );
    let images: Vec<_> = Look::ALL
        .into_iter()
        .map(|look| {
            let frame = match look {
                Look::Skin(skin) => {
                    opencreative_render::panels::render(&snap, &history, layout, skin.into(), w, h)
                }
                Look::Gauges => opencreative_render::monitor::render(&snap, &clock, w, h),
            };
            let rgba = crate::panel_colors(frame.data(), w);
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba)
        })
        .collect();
    let thumbs = &mut app.skin_thumbs;
    if thumbs.textures.len() == images.len() {
        for (texture, image) in thumbs.textures.iter_mut().zip(images) {
            texture.set(image, crate::PIXEL_TEXTURE);
        }
    } else {
        thumbs.textures = images
            .into_iter()
            .enumerate()
            .map(|(i, image)| ctx.load_texture(format!("skin-{i}"), image, crate::PIXEL_TEXTURE))
            .collect();
    }
    thumbs.rendered = Some(Instant::now());
    ctx.request_repaint_after(THUMB_REFRESH);
}

fn metric_combo(ui: &mut egui::Ui, id: &str, metric: &mut Metric, claude: bool) -> bool {
    let before = *metric;
    egui::ComboBox::from_id_salt(id)
        .width(220.0)
        .selected_text(metric.name())
        .show_ui(ui, |ui| {
            // Claude readings only when the integration is turned on.
            for m in Metric::ALL.into_iter().filter(|m| claude || !m.is_claude()) {
                ui.selectable_value(metric, m, m.name());
            }
        });
    *metric != before
}

fn multimedia(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let scene = app.applied.scene.clone();
    let mut source = if matches!(scene, SceneConfig::Solid { .. }) {
        Source::Color
    } else {
        Source::Media
    };

    section(ui, "Source");
    if segmented(
        ui,
        &mut source,
        &[
            (Source::Media, "Images and video"),
            (Source::Color, "Color"),
        ],
    ) {
        match source {
            Source::Color => app.set_scene(SceneConfig::Solid {
                color: Rgb(SWATCHES[0]),
            }),
            Source::Media => {
                let (items, secs) = app.playlist();
                if items.is_empty() {
                    let ctx = ui.ctx().clone();
                    app.browse_for_media(&ctx);
                } else {
                    app.set_playlist(items, secs, false);
                }
            }
        }
        return;
    }
    ui.add_space(10.0);

    if let SceneConfig::Solid { color } = scene {
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
        return;
    }
    playlist(app, ui);
}

/// The playlist: one row per item, the selected item's fit and zoom, how
/// long each shows, and ways to add more.
fn playlist(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let (mut items, mut secs) = app.playlist();
    if items.is_empty() {
        drop_zone(app, ui);
        ui.add_space(10.0);
        add_url_row(app, ui);
        return;
    }
    app.selected_item = app.selected_item.min(items.len() - 1);

    // Row actions are applied after the loop.
    let mut action = None;
    for (i, item) in items.iter().enumerate() {
        let selected = i == app.selected_item;
        let frame = egui::Frame::new()
            .fill(if selected {
                p.raised
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::new(
                1.0,
                if selected {
                    p.accent.gamma_multiply(0.5)
                } else {
                    p.border
                },
            ))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(10, 6));
        let row = frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let kind = if item.url.is_some() {
                    Icon::Link
                } else if item
                    .path
                    .as_deref()
                    .is_some_and(opencreative_render::media::is_video_path)
                {
                    Icon::Film
                } else {
                    Icon::Image
                };
                icons::show(ui, kind, 16.0, if selected { p.accent } else { p.muted });
                ui.add(
                    egui::Label::new(egui::RichText::new(item.label()).color(p.text)).truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Trash, "Take out of the playlist").clicked() {
                        action = Some(RowAction::TakeOut(i));
                    }
                    if i + 1 < items.len() && icon_button(ui, Icon::Down, "Move down").clicked() {
                        action = Some(RowAction::Swap(i, i + 1));
                    }
                    if i > 0 && icon_button(ui, Icon::Up, "Move up").clicked() {
                        action = Some(RowAction::Swap(i - 1, i));
                    }
                });
            });
        });
        let click = ui.interact(row.response.rect, ui.id().with(("item", i)), Sense::click());
        if click
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.selected_item = i;
        }
        ui.add_space(4.0);
    }
    match action {
        Some(RowAction::TakeOut(i)) => {
            items.remove(i);
            if items.is_empty() {
                return app.set_scene(SceneConfig::Solid {
                    color: Rgb(SWATCHES[0]),
                });
            }
            return app.set_playlist(items, secs, false);
        }
        Some(RowAction::Swap(a, b)) => {
            items.swap(a, b);
            app.selected_item = if app.selected_item == a { b } else { a };
            return app.set_playlist(items, secs, false);
        }
        None => {}
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let browsing = app.picking_file.is_some();
        if ui
            .add_enabled(!browsing, egui::Button::new("Add files…"))
            .clicked()
        {
            app.browse_for_media(&ctx);
        }
        ui.label(
            egui::RichText::new("or drop them on the window")
                .small()
                .color(p.faint),
        );
    });
    ui.add_space(6.0);
    add_url_row(app, ui);

    // The selected item's fit and crop.
    ui.add_space(12.0);
    let selected = app.selected_item;
    let item = &mut items[selected];
    section(ui, &format!("“{}”", truncate(&item.label(), 40)));
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Fit").color(p.muted));
        changed |= segmented(
            ui,
            &mut item.fit,
            &[
                (FitConfig::Cover, "Fill"),
                (FitConfig::Contain, "Fit"),
                (FitConfig::Stretch, "Stretch"),
            ],
        );
    });
    let mut draft = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Zoom").color(p.muted));
        let zoom = ui.add(
            egui::Slider::new(
                &mut item.zoom,
                1.0..=opencreative_render::media::Crop::MAX_ZOOM,
            )
            .logarithmic(true)
            .custom_formatter(|v, _| format!("{v:.1}×")),
        );
        draft |= zoom.changed();
        let cropped = (item.zoom - 1.0).abs() > f32::EPSILON || item.focus != [0.5, 0.5];
        if cropped && ui.small_button("Reset").clicked() {
            item.zoom = 1.0;
            item.focus = [0.5, 0.5];
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("Drag the preview to move the picture; scroll on it to zoom.")
            .small()
            .color(p.faint),
    );
    if items.len() > 1 {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Each item shows for").color(p.muted));
            draft |= ui
                .add(
                    egui::Slider::new(&mut secs, 2.0..=300.0)
                        .logarithmic(true)
                        .custom_formatter(|v, _| format!("{v:.0} s")),
                )
                .changed();
        });
    }
    if changed {
        app.set_playlist(items.clone(), secs, false);
    } else if draft {
        app.set_playlist(items.clone(), secs, true);
    }
    tool_notices(app, ui, &items);
}

enum RowAction {
    TakeOut(usize),
    Swap(usize, usize),
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    }
}

/// A field for a YouTube (or direct video) address.
fn add_url_row(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut app.url_input)
                .hint_text("YouTube or video link")
                .desired_width((ui.available_width() - 100.0).max(160.0)),
        );
        let url = app.url_input.trim().to_owned();
        let valid = url.starts_with("https://") || url.starts_with("http://");
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let clicked = ui
            .add_enabled(valid, egui::Button::new("Add link"))
            .clicked();
        if valid && (clicked || enter) {
            app.add_url(url);
            app.url_input.clear();
        }
    });
}

/// Explains missing helper programs for videos and web links.
fn tool_notices(app: &mut App, ui: &mut egui::Ui, items: &[opencreative_config::MediaItem]) {
    use opencreative_render::video::tool;
    let p = Palette::of(ui.ctx());
    let has_video = items.iter().any(|i| {
        i.url.is_some()
            || i.path
                .as_deref()
                .is_some_and(opencreative_render::media::is_video_path)
    });
    let has_url = items.iter().any(|i| i.url.is_some());
    let notice = |ui: &mut egui::Ui, text: &str| {
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            icons::show(ui, Icon::Help, 15.0, p.warn);
            ui.label(egui::RichText::new(text).color(p.muted));
        });
    };
    if has_video && tool("ffmpeg").is_none() {
        notice(
            ui,
            "Videos need ffmpeg. Install it with `sudo apt install ffmpeg` (or your \
             distribution's package manager).",
        );
        if ui.small_button("Copy command").clicked() {
            ui.ctx().copy_text("sudo apt install ffmpeg".to_owned());
            app.notify("Command copied.");
        }
    }
    if has_url && tool("yt-dlp").is_none() {
        notice(
            ui,
            "YouTube links need yt-dlp. OpenCreative can download the current release \
             for you (about 3 MB, into its own folder).",
        );
        let busy = app.task.is_some();
        if ui
            .add_enabled(!busy, egui::Button::new("Download yt-dlp"))
            .clicked()
        {
            let ctx = ui.ctx().clone();
            app.run_task(&ctx, "Downloading yt-dlp", crate::service::download_ytdlp);
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
                icons::show(ui, Icon::Upload, 26.0, p.accent);
                ui.label(
                    egui::RichText::new("Drop images, GIFs or videos anywhere on this window")
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new(
                        "PNG, JPEG, WebP, BMP, GIF, MP4, WebM, MKV… Several play one after another.",
                    )
                    .small()
                    .color(p.muted),
                );
                ui.add_space(4.0);
                let browsing = app.picking_file.is_some();
                if ui
                    .add_enabled(!browsing, egui::Button::new("Browse…"))
                    .clicked()
                {
                    app.browse_for_media(&ctx);
                }
            });
        });
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, p.raised);
    }
    icons::paint(ui, rect.shrink(7.0), icon, p.muted);
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
