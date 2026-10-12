//! The cooler page: what its screen shows, laid out like the vendor app's
//! device page (display controls on the left, preview and content on the
//! right). Every change applies immediately.

use eframe::egui::{self, Color32, Sense, Stroke, vec2};
use std::time::{Duration, Instant};

use opencreative_config::{BarStyleConfig, FitConfig, Metric, Rgb, SceneConfig, SkinConfig};
use opencreative_render::panels::Layout;

use crate::icons::{self, Icon};
use crate::library::MediaKind;
use crate::theme::{self, Palette};
use crate::widgets::{
    Swatch, card_look, pill, section, segmented, segmented_width, swatches, toggle,
};
use crate::{App, Page, is_digital, is_music};

/// The settings column under the preview.
const COLUMN_WIDTH: f32 = 640.0;
/// The label side of a settings row.
const LABEL_WIDTH: f32 = 128.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Digital,
    Multimedia,
    Music,
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
    } else if is_music(&app.applied.scene) {
        Mode::Music
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
    });
    ui.add_space(14.0);

    if app.device.is_none() {
        notice(app, ui);
        ui.add_space(12.0);
    }

    // The stage: the screen alone, the mode under it, then one column of
    // settings, what it shows first and how the screen behaves after.
    ui.add_space(8.0);
    preview(app, ui);
    ui.add_space(18.0);
    let modes: Vec<(Mode, &str)> = Mode::ALL.iter().map(|m| (*m, m.name())).collect();
    let mut picked = mode;
    centered(ui, segmented_width(ui, &modes), |ui| {
        segmented(ui, &mut picked, &modes);
    });
    if picked != mode {
        mode = picked;
        let scene = match mode {
            Mode::Digital => app.digital_scene.clone(),
            Mode::Multimedia => app.media_scene.clone(),
            Mode::Music => app.music_scene.clone(),
        };
        app.set_scene(scene);
    }
    ui.add_space(28.0);

    let width = ui.available_width().min(COLUMN_WIDTH);
    centered(ui, width, |ui| {
        ui.vertical(|ui| {
            ui.set_width(width);
            match mode {
                Mode::Digital => digital(app, ui),
                Mode::Multimedia => multimedia(app, ui),
                Mode::Music => music(app, ui),
            }
            divider(ui);
            display_controls(app, ui, mode);
        });
    });
    ui.add_space(56.0);
}

impl Mode {
    const ALL: [Mode; 3] = [Mode::Digital, Mode::Multimedia, Mode::Music];

    fn name(self) -> &'static str {
        match self {
            Mode::Digital => "Digital",
            Mode::Multimedia => "Multimedia",
            Mode::Music => "Music",
        }
    }
}

fn notice(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    egui::Frame::new()
        .fill(p.warn.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.5)))
        .corner_radius(theme::RADIUS)
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
    row(ui, "Brightness", |ui| {
        let mut level = app.brightness;
        let slider = slider_row(ui, &mut level, 0..=100, |v| format!("{v}%"));
        app.brightness = level;
        // Send once the user lets go, not on every step of a drag.
        if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
            app.set_brightness(app.brightness);
        }
    });
    row(ui, "Zen mode", |ui| {
        trailing(ui, |ui| {
            let mut zen = app.zen;
            let response = toggle(ui, &mut zen).on_hover_text(
                "Turns the screen off. Your layout and brightness are kept for when you turn it back on.",
            );
            if response.changed() {
                app.set_zen(zen);
            }
        });
    });
    // Pictures and videos keep their own colors.
    if mode != Mode::Multimedia {
        row(ui, "Colors", |ui| {
            let mut theme = app.applied.screen_theme;
            if segmented(
                ui,
                &mut theme,
                &[
                    (opencreative_config::ScreenTheme::Dark, "Dark"),
                    (opencreative_config::ScreenTheme::Light, "Light"),
                ],
            ) {
                app.set_screen_theme(theme);
            }
        });
    }
    row(ui, "Rotation", |ui| {
        let mut rotation = app.applied.rotation;
        if segmented(
            ui,
            &mut rotation,
            &[(0, "0°"), (90, "90°"), (180, "180°"), (270, "270°")],
        ) {
            app.set_rotation(rotation);
        }
    });
    if mode == Mode::Digital
        && let Some(mut secs) = interval(&app.applied.scene)
    {
        row(ui, "Refresh every", |ui| {
            let slider = slider_row(ui, &mut secs, 0.5..=5.0, |v| format!("{v:.1} s"));
            if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                let mut scene = app.applied.scene.clone();
                set_interval(&mut scene, secs);
                app.set_scene(scene);
            }
        });
    }
}

/// One settings line: a muted label on the left, its control beside it.
fn row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = Palette::of(ui.ctx());
    let inner = ui
        .horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(LABEL_WIDTH, 32.0), Sense::hover());
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                label,
                theme::regular(14.0),
                p.muted,
            );
            ui.vertical(|ui| {
                ui.set_min_height(32.0);
                add(ui)
            })
            .inner
        })
        .inner;
    ui.add_space(6.0);
    inner
}

/// Puts a control at the right end of a row.
fn trailing(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 32.0),
        egui::Layout::right_to_left(egui::Align::Center),
        add,
    );
}

/// A slider across the row with its value at the end.
fn slider_row<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    text: impl Fn(N) -> String,
) -> egui::Response {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(80.0);
        let slider = ui.add(egui::Slider::new(value, range).show_value(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(text(*value)).color(p.text));
        });
        slider
    })
    .inner
}

/// A hairline between groups of rows.
fn divider(ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    ui.add_space(10.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.border));
    ui.add_space(16.0);
}

/// Lays out `width` points of content centered in the available width.
fn centered(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        add(ui);
    });
}

fn preview(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let (w, h) = app.preview.size;
    let pad = 10.0;
    // Whole-number zoom in physical pixels keeps every panel pixel sharp.
    let ppp = ui.ctx().pixels_per_point();
    let room = (ui.available_width() - 2.0 * pad) * ppp;
    // Whole-number zoom, never larger than the panel's real size in points;
    // a column too narrow for one physical pixel per pixel scales down.
    let fit = room / w.max(1) as f32;
    let zoom = if fit >= 1.0 {
        fit.floor().min(ppp.round().max(1.0))
    } else {
        fit
    };
    let screen = vec2(w as f32, h as f32) * zoom / ppp;
    ui.vertical_centered(|ui| {
        let (bezel, response) =
            ui.allocate_exact_size(screen + vec2(2.0 * pad, 2.0 * pad), Sense::drag());
        pan_and_zoom(app, ui, &response, screen);
        let painter = ui.painter();
        painter.rect_filled(bezel, theme::RADIUS as f32, p.bezel);
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
    if is_digital(&app.applied.scene)
        || matches!(
            app.applied.scene,
            SceneConfig::Solid { .. } | SceneConfig::Music { .. }
        )
    {
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

    if let Some(next) = row(ui, "Skin", |ui| skin_gallery(app, ui, &scene, secs)) {
        app.set_scene(next);
        return;
    }
    divider(ui);

    match scene {
        SceneConfig::Panels {
            skin,
            mut primary,
            mut secondary,
            interval_secs,
        } => {
            let claude = app.prefs.claude_usage;
            // With only the CPU package measurable, PC power reads "--".
            let no_pc_power = app
                .history
                .latest()
                .is_some_and(|s| s.system_power_w.is_none() && s.cpu_power_w.is_some());
            let choices = Choices {
                claude,
                no_pc_power,
            };
            let mut changed = false;
            row(ui, "Main reading", |ui| {
                changed |= metric_combo(ui, "primary", &mut primary, choices);
            });
            row(ui, "Second reading", |ui| {
                changed |= metric_combo(ui, "secondary-1", &mut secondary[0], choices);
            });
            row(ui, "Third reading", |ui| {
                // The HUD skin charts network traffic in that spot instead.
                let shown = skin != SkinConfig::Hud;
                ui.add_enabled_ui(shown, |ui| {
                    changed |= metric_combo(ui, "secondary-2", &mut secondary[1], choices);
                })
                .response
                .on_disabled_hover_text("The HUD skin shows network speeds here.");
            });
            row(ui, "Presets", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    let presets = PRESETS.into_iter().chain(claude.then_some(CLAUDE_PRESET));
                    for (name, main, others) in presets {
                        let selected = primary == main && secondary == others;
                        let detail = format!(
                            "{} (main), {}, {}",
                            main.name(),
                            others[0].name(),
                            others[1].name()
                        );
                        if pill(ui, name, selected).on_hover_text(detail).clicked() && !selected {
                            primary = main;
                            secondary = others;
                            changed = true;
                        }
                    }
                });
            });
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
    /// The size they were rendered for, in points.
    size: egui::Vec2,
    /// The screen colors they were rendered in.
    theme: opencreative_config::ScreenTheme,
}

const THUMB_REFRESH: Duration = Duration::from_secs(2);
/// The largest thumbnail; they shrink to keep the gallery on one row.
const THUMB_SIZE: egui::Vec2 = vec2(176.0, 132.0);
/// Each card's margin and border around its thumbnail.
const THUMB_CHROME: f32 = 2.0 * 8.0 + 2.0 * 2.0;

/// The thumbnail size that fits every look side by side in `width`.
fn thumb_size(width: f32, gap: f32) -> egui::Vec2 {
    let n = Look::ALL.len() as f32;
    let w = ((width - gap * (n - 1.0)) / n - THUMB_CHROME)
        .clamp(80.0, THUMB_SIZE.x)
        .floor();
    vec2(w, (w * THUMB_SIZE.y / THUMB_SIZE.x).round())
}

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
    let size = thumb_size(ui.available_width(), ui.spacing().item_spacing.x);
    refresh_thumbs(app, ui.ctx(), &layout, size);

    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (look, texture) in Look::ALL.into_iter().zip(&app.skin_thumbs.textures) {
            let selected = look == current;
            let response = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .id_salt(("skin", look.name()))
                        .sense(Sense::click()),
                    |ui| {
                        let (fill, stroke) = card_look(
                            ui,
                            ui.id(),
                            selected,
                            ui.response().hovered(),
                            Color32::TRANSPARENT,
                            Color32::TRANSPARENT,
                        );
                        egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(theme::RADIUS)
                            .inner_margin(8)
                            .show(ui, |ui| {
                                // Keep each card as narrow as its thumbnail.
                                ui.set_max_width(size.x);
                                ui.vertical_centered(|ui| {
                                    ui.add(
                                        egui::Image::new((texture.id(), size))
                                            .corner_radius(theme::SMALL_RADIUS),
                                    );
                                    ui.label(egui::RichText::new(look.name()).color(if selected {
                                        p.accent
                                    } else {
                                        p.text
                                    }));
                                });
                            });
                    },
                )
                .response
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

fn refresh_thumbs(app: &mut App, ctx: &egui::Context, layout: &Layout, size: egui::Vec2) {
    let thumbs = &mut app.skin_thumbs;
    let fresh = thumbs
        .rendered
        .is_some_and(|at| at.elapsed() < THUMB_REFRESH);
    let theme = app.applied.screen_theme;
    if fresh
        && thumbs.size == size
        && thumbs.theme == theme
        && thumbs.textures.len() == Look::ALL.len()
    {
        return;
    }
    let snap = app.history.latest().cloned().unwrap_or_default();
    let history = app.history.samples();
    let clock = chrono::Local::now().format("%H:%M").to_string();
    // Render at the thumbnail's exact pixel size so nothing is resampled.
    let ppp = ctx.pixels_per_point();
    let (w, h) = ((size.x * ppp).round() as u32, (size.y * ppp).round() as u32);
    let images: Vec<_> = Look::ALL
        .into_iter()
        .map(|look| {
            let mut frame = match look {
                Look::Skin(skin) => {
                    opencreative_render::panels::render(&snap, &history, layout, skin.into(), w, h)
                }
                Look::Gauges => opencreative_render::monitor::render(&snap, &clock, w, h),
            };
            opencreative_render::shade::apply(&mut frame, theme.into());
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
    thumbs.size = size;
    thumbs.theme = theme;
    ctx.request_repaint_after(THUMB_REFRESH);
}

/// What the reading pickers offer.
#[derive(Clone, Copy)]
struct Choices {
    /// Claude readings, when the integration is turned on.
    claude: bool,
    /// This PC has no whole-system power sensor.
    no_pc_power: bool,
}

fn metric_combo(ui: &mut egui::Ui, id: &str, metric: &mut Metric, choices: Choices) -> bool {
    let before = *metric;
    let name = |m: Metric| {
        if m == Metric::SystemPower && choices.no_pc_power {
            "PC power (not measurable here)"
        } else {
            m.name()
        }
    };
    egui::ComboBox::from_id_salt(id)
        .width(220.0)
        .selected_text(name(*metric))
        .show_ui(ui, |ui| {
            for m in Metric::ALL
                .into_iter()
                .filter(|m| choices.claude || !m.is_claude())
            {
                let option = ui.selectable_value(metric, m, name(m));
                if m == Metric::SystemPower && choices.no_pc_power {
                    option.on_hover_text(
                        "This PC has no whole-system power sensor (RAPL psys), no memory \
                         counter and no discrete GPU, so only CPU power can be measured.",
                    );
                }
            }
        });
    *metric != before
}

/// Music bars: their layout, color and how many.
fn music(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    let SceneConfig::Music {
        mut style,
        color,
        mut rainbow,
        mut bars,
    } = app.music_scene.clone()
    else {
        return;
    };
    let mut rgb = color.0;
    let mut changed = false;

    row(ui, "Style", |ui| {
        changed |= segmented(
            ui,
            &mut style,
            &[
                (BarStyleConfig::Bars, "Rising"),
                (BarStyleConfig::Mirror, "Mirrored"),
            ],
        );
    });
    row(ui, "Bars", |ui| {
        let mut count = bars;
        let slider = slider_row(ui, &mut count, 8..=48, |v| v.to_string());
        // Whole steps of four.
        bars = (count / 4) * 4;
        changed |= slider.drag_stopped() || (slider.changed() && !slider.dragged());
    });
    let current = if rainbow {
        Swatch::Rainbow
    } else {
        Swatch::Color(rgb)
    };
    match row(ui, "Color", |ui| {
        swatches(ui, "music", current, &SWATCHES, true)
    }) {
        Some(Swatch::Rainbow) => {
            rainbow = true;
            changed = true;
        }
        Some(Swatch::Color(c)) => {
            rainbow = false;
            rgb = c;
            changed = true;
        }
        None => {}
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "The bars follow whatever your speakers or headphones are playing. \
             If the screen says NO AUDIO, check that PipeWire (pw-record) or \
             PulseAudio (parec) is available.",
        )
        .small()
        .color(p.faint),
    );

    if changed {
        app.set_scene(SceneConfig::Music {
            style,
            color: Rgb(rgb),
            rainbow,
            bars,
        });
    }
}

fn multimedia(app: &mut App, ui: &mut egui::Ui) {
    let scene = app.applied.scene.clone();

    let mut tab = app.media_tab;
    let mut tabs: Vec<(Option<MediaKind>, &str)> = MediaKind::ALL
        .iter()
        .map(|k| (Some(*k), k.name()))
        .collect();
    tabs.push((None, "Color"));
    if row(ui, "Source", |ui| segmented(ui, &mut tab, &tabs)) {
        app.switch_media_tab(tab);
        match tab {
            None => app.set_scene(SceneConfig::Solid {
                color: Rgb(SWATCHES[0]),
            }),
            Some(_) => {
                // An empty tab shows its drop zone; picking files is up to
                // the user.
                let (items, secs) = app.playlist();
                if !items.is_empty() {
                    app.set_playlist(items, secs, false);
                }
            }
        }
        return;
    }
    ui.add_space(10.0);

    let Some(kind) = app.media_tab else {
        let rgb = match scene {
            SceneConfig::Solid { color } => color.0,
            _ => SWATCHES[0],
        };
        let picked = match swatches(ui, "solid", Swatch::Color(rgb), &SWATCHES, false) {
            Some(Swatch::Color(c)) => c,
            _ => rgb,
        };
        if picked != rgb {
            app.set_scene(SceneConfig::Solid { color: Rgb(picked) });
        }
        return;
    };
    playlist(app, ui, kind);
}

/// The playlist: one row per item, the selected item's fit and zoom, how
/// long each shows, and ways to add more.
fn playlist(app: &mut App, ui: &mut egui::Ui, kind: MediaKind) {
    let p = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let (mut items, mut secs) = app.playlist();
    if kind == MediaKind::Youtube {
        ytdlp_notice(app, ui);
    }
    if items.is_empty() {
        if kind == MediaKind::Youtube {
            add_url_row(app, ui);
        } else {
            drop_zone(app, ui, kind);
        }
        return;
    }
    app.selected_item = app.selected_item.min(items.len() - 1);
    app.selection.retain(|&i| i < items.len());

    // Row actions are applied after the loop.
    let mut action = None;
    let modifiers = ui.input(|i| i.modifiers);
    for (i, item) in items.iter().enumerate() {
        let lit = i == app.selected_item || app.selection.contains(&i);
        let frame = egui::Frame::new()
            .fill(if lit { p.raised } else { Color32::TRANSPARENT })
            .stroke(Stroke::new(
                1.0,
                if lit {
                    p.accent.gamma_multiply(0.5)
                } else {
                    p.border
                },
            ))
            .corner_radius(theme::SMALL_RADIUS)
            .inner_margin(egui::Margin::symmetric(10, 6));
        // The row's click is registered before its buttons, so the buttons
        // (on top) get their own clicks.
        let row = ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("item", i))
                .sense(Sense::click()),
            |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let icon = match kind {
                            MediaKind::Youtube => Icon::Link,
                            MediaKind::Video => Icon::Film,
                            MediaKind::Image | MediaKind::Gif => Icon::Image,
                        };
                        icons::show(ui, icon, 16.0, if lit { p.accent } else { p.muted });
                        ui.add(
                            egui::Label::new(egui::RichText::new(item.label()).color(p.text))
                                .truncate(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, Icon::Trash, "Take out of the playlist").clicked() {
                                action = Some(RowAction::TakeOut(vec![i]));
                            }
                            if i + 1 < items.len()
                                && icon_button(ui, Icon::Down, "Move down").clicked()
                            {
                                action = Some(RowAction::Swap(i, i + 1));
                            }
                            if i > 0 && icon_button(ui, Icon::Up, "Move up").clicked() {
                                action = Some(RowAction::Swap(i - 1, i));
                            }
                        });
                    });
                })
            },
        );
        if row
            .response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            pick_row(app, i, modifiers);
        }
        ui.add_space(4.0);
    }

    // Ctrl+A picks every row and Delete takes the picked ones out, unless
    // a text field (the link box) has the keyboard.
    let typing = ctx.egui_wants_keyboard_input();
    if !typing {
        let (all, delete) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
            )
        });
        if all {
            app.selection = (0..items.len()).collect();
        }
        if delete {
            action = Some(RowAction::TakeOut(picked_rows(app)));
        }
    }

    if app.selection.len() > 1 {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{} selected", app.selection.len())).color(p.text),
            );
            if ui.button("Remove selected").clicked() {
                action = Some(RowAction::TakeOut(picked_rows(app)));
            }
            if ui.button("Clear selection").clicked() {
                app.selection.clear();
            }
        });
    } else {
        ui.label(
            egui::RichText::new("Ctrl-click or Shift-click to select several, then press Delete.")
                .small()
                .color(p.faint),
        );
    }

    match action {
        Some(RowAction::TakeOut(rows)) => {
            for &i in rows.iter().rev() {
                items.remove(i);
            }
            let before = rows.iter().filter(|&&i| i < app.selected_item).count();
            app.selected_item = app.selected_item.saturating_sub(before);
            app.selection.clear();
            return app.set_playlist(items, secs, false);
        }
        Some(RowAction::Swap(a, b)) => {
            items.swap(a, b);
            app.selected_item = if app.selected_item == a { b } else { a };
            let (had_a, had_b) = (app.selection.remove(&a), app.selection.remove(&b));
            if had_a {
                app.selection.insert(b);
            }
            if had_b {
                app.selection.insert(a);
            }
            return app.set_playlist(items, secs, false);
        }
        None => {}
    }

    ui.add_space(6.0);
    if kind == MediaKind::Youtube {
        add_url_row(app, ui);
    } else {
        ui.horizontal(|ui| {
            let browsing = app.picking_file.is_some();
            if ui
                .add_enabled(
                    !browsing,
                    egui::Button::new(format!("Add {}…", kind.noun(2))),
                )
                .clicked()
            {
                app.browse_for_media(&ctx, kind);
            }
            ui.label(
                egui::RichText::new("or drop them on the window")
                    .small()
                    .color(p.faint),
            );
        });
    }

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
    tool_notices(app, ui, kind);
}

/// A click on row `i`: plain picks just it, Ctrl toggles it, Shift picks
/// the range from the focused row.
fn pick_row(app: &mut App, i: usize, modifiers: egui::Modifiers) {
    if modifiers.shift {
        let (a, b) = (app.selected_item.min(i), app.selected_item.max(i));
        if !modifiers.command {
            app.selection.clear();
        }
        app.selection.extend(a..=b);
    } else if modifiers.command {
        if app.selection.is_empty() {
            app.selection.insert(app.selected_item);
        }
        if !app.selection.remove(&i) {
            app.selection.insert(i);
        }
        app.selected_item = i;
    } else {
        app.selection.clear();
        app.selected_item = i;
    }
}

/// The rows Delete or "Remove selected" takes out, in order: the picked
/// ones, or else the focused one.
fn picked_rows(app: &App) -> Vec<usize> {
    if app.selection.is_empty() {
        vec![app.selected_item]
    } else {
        app.selection.iter().copied().collect()
    }
}

enum RowAction {
    /// Rows to take out, in ascending order.
    TakeOut(Vec<usize>),
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
            let ctx = ui.ctx().clone();
            app.add_url(&ctx, url);
            app.url_input.clear();
        }
    });
}

/// Explains missing helper programs for videos and web links.
fn tool_notices(app: &mut App, ui: &mut egui::Ui, kind: MediaKind) {
    use opencreative_render::video::tool;
    let p = Palette::of(ui.ctx());
    let has_video = matches!(kind, MediaKind::Video | MediaKind::Youtube);
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
}

/// YouTube links need yt-dlp: offer to download it when it's missing.
fn ytdlp_notice(app: &mut App, ui: &mut egui::Ui) {
    let p = Palette::of(ui.ctx());
    if opencreative_render::video::tool("yt-dlp").is_some() {
        return;
    }
    egui::Frame::new()
        .fill(p.warn.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.5)))
        .corner_radius(theme::RADIUS)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                icons::show(ui, Icon::Help, 15.0, p.warn);
                ui.label(
                    egui::RichText::new(
                        "YouTube links need yt-dlp. OpenCreative can download the current \
                         release for you (about 3 MB, into its own folder).",
                    )
                    .color(p.text),
                );
            });
            ui.add_space(6.0);
            let busy = app.task.is_some();
            if ui
                .add_enabled(!busy, egui::Button::new("Download yt-dlp"))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                app.run_task(&ctx, "Downloading yt-dlp", crate::service::download_ytdlp);
            }
        });
    ui.add_space(10.0);
}

fn drop_zone(app: &mut App, ui: &mut egui::Ui, kind: MediaKind) {
    let p = Palette::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let formats = kind
        .extensions()
        .iter()
        .map(|e| e.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(", ");
    egui::Frame::new()
        .fill(p.accent_soft)
        .stroke(Stroke::new(1.0, p.accent.gamma_multiply(0.6)))
        .corner_radius(theme::RADIUS)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                icons::show(ui, Icon::Upload, 26.0, p.accent);
                ui.label(
                    egui::RichText::new(format!("Drop {} anywhere on this window", kind.noun(2)))
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new(format!("{formats}. Several play one after another."))
                        .small()
                        .color(p.muted),
                );
                ui.add_space(4.0);
                let browsing = app.picking_file.is_some();
                if ui
                    .add_enabled(!browsing, egui::Button::new("Browse…"))
                    .clicked()
                {
                    app.browse_for_media(&ctx, kind);
                }
            });
        });
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme::SMALL_RADIUS as f32, p.raised);
    }
    let tint = if response.hovered() { p.text } else { p.muted };
    icons::paint(ui, rect.shrink(7.0), icon, tint);
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
    fn thumbnails_shrink_to_fit_one_row() {
        let wide = thumb_size(2000.0, 10.0);
        assert_eq!(wide, THUMB_SIZE);
        let narrow = thumb_size(540.0, 10.0);
        let row = 3.0 * (narrow.x + THUMB_CHROME) + 2.0 * 10.0;
        assert!(row <= 540.0, "{row}");
    }

    /// A button inside a clickable playlist row must get its own click,
    /// and a click beside it must still pick the row.
    #[test]
    fn row_buttons_get_their_own_clicks() {
        let ctx = egui::Context::default();
        // As in the app: labels aren't text-selectable.
        ctx.global_style_mut(|s| s.interaction.selectable_labels = false);
        let time = std::cell::Cell::new(0.0);
        let frame = |events: Vec<egui::Event>| {
            // Clicks are timed, so the clock has to move.
            time.set(time.get() + 0.05);
            let mut hits = (false, false, egui::Rect::NOTHING);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(400.0, 200.0),
                )),
                time: Some(time.get()),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let row = ui.scope_builder(
                    egui::UiBuilder::new().id_salt("row").sense(Sense::click()),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label("clip.mp4");
                            let button = ui.button("trash");
                            hits.0 |= button.clicked();
                            hits.2 = button.rect;
                        });
                    },
                );
                // The UI may run twice in one frame; a click counts once.
                hits.1 |= row.response.clicked();
            });
            output.textures_delta.clear();
            hits
        };
        let button_event = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        // Move, press and release on separate frames, like a real mouse.
        let click = |at: egui::Pos2| {
            let _ = frame(vec![egui::Event::PointerMoved(at)]);
            let _ = frame(vec![button_event(at, true)]);
            frame(vec![button_event(at, false)])
        };
        let (_, _, button) = frame(Vec::new());
        let (on_button, on_row, _) = click(button.center());
        assert!(on_button && !on_row, "button {on_button}, row {on_row}");
        let (on_button, on_row, _) = click(egui::pos2(4.0, button.center().y));
        assert!(!on_button && on_row, "button {on_button}, row {on_row}");
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
