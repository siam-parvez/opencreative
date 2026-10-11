//! Look and feel: a light theme with a teal accent (and a dark variant),
//! IBM Plex Sans, and card-based layout helpers.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    Stroke, TextStyle, Theme, Visuals,
};

/// Named colors for one theme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub card: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub track: Color32,
    pub control: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    /// The device frame around the LCD preview.
    pub bezel: Color32,
}

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf3, 0xf5, 0xf7),
    card: Color32::from_rgb(0xff, 0xff, 0xff),
    border: Color32::from_rgb(0xe4, 0xe8, 0xec),
    text: Color32::from_rgb(0x1d, 0x23, 0x2b),
    muted: Color32::from_rgb(0x6a, 0x73, 0x80),
    faint: Color32::from_rgb(0x9a, 0xa3, 0xae),
    accent: Color32::from_rgb(0x0d, 0x9b, 0x94),
    accent_soft: Color32::from_rgb(0xe3, 0xf4, 0xf3),
    track: Color32::from_rgb(0xe9, 0xec, 0xef),
    control: Color32::from_rgb(0xf6, 0xf7, 0xf9),
    good: Color32::from_rgb(0x22, 0xc5, 0x5e),
    warn: Color32::from_rgb(0xf5, 0x9e, 0x0b),
    bad: Color32::from_rgb(0xdc, 0x26, 0x26),
    bezel: Color32::from_rgb(0x2a, 0x2e, 0x34),
};

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x0f, 0x12, 0x17),
    card: Color32::from_rgb(0x17, 0x1b, 0x22),
    border: Color32::from_rgb(0x26, 0x2c, 0x35),
    text: Color32::from_rgb(0xe6, 0xe9, 0xee),
    muted: Color32::from_rgb(0x9b, 0xa4, 0xb0),
    faint: Color32::from_rgb(0x6b, 0x74, 0x80),
    accent: Color32::from_rgb(0x2d, 0xc4, 0xb6),
    accent_soft: Color32::from_rgb(0x13, 0x2e, 0x2d),
    track: Color32::from_rgb(0x2a, 0x30, 0x3a),
    control: Color32::from_rgb(0x1e, 0x23, 0x2b),
    good: Color32::from_rgb(0x34, 0xd3, 0x99),
    warn: Color32::from_rgb(0xfb, 0xbf, 0x24),
    bad: Color32::from_rgb(0xf8, 0x71, 0x71),
    bezel: Color32::from_rgb(0x05, 0x06, 0x08),
};

impl Palette {
    pub fn of(ctx: &egui::Context) -> Self {
        if ctx.theme() == Theme::Dark {
            DARK
        } else {
            LIGHT
        }
    }
}

const MEDIUM: &str = "medium";
const SEMIBOLD: &str = "semibold";

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Installs fonts, text sizes and both light and dark visuals.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 3] = [
        (
            "plex",
            include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf"),
        ),
        (
            MEDIUM,
            include_bytes!("../../../assets/fonts/IBMPlexSans-Medium.ttf"),
        ),
        (
            SEMIBOLD,
            include_bytes!("../../../assets/fonts/IBMPlexSans-SemiBold.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // Keep egui's fonts as fallbacks for symbols Plex lacks.
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |first: &str| {
        let mut list = vec![first.to_owned()];
        list.extend(fallbacks.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("plex"));
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM.into()), family(MEDIUM));
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), family(SEMIBOLD));
    ctx.set_fonts(fonts);

    for (theme, palette) in [(Theme::Light, LIGHT), (Theme::Dark, DARK)] {
        ctx.set_visuals_of(theme, visuals(&palette, theme == Theme::Dark));
        ctx.style_mut_of(theme, |style| {
            style.text_styles = [
                (TextStyle::Heading, semibold(20.0)),
                (TextStyle::Body, regular(14.0)),
                (TextStyle::Button, regular(14.0)),
                (TextStyle::Small, regular(12.0)),
                (TextStyle::Monospace, FontId::monospace(13.0)),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            style.spacing.interact_size.y = 30.0;
            style.spacing.slider_width = 240.0;
            style.spacing.combo_width = 200.0;
            // A scrollbar with its own gutter, so it never covers card edges.
            style.spacing.scroll = egui::style::ScrollStyle::solid();
        });
    }
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.extreme_bg_color = p.control;
    v.faint_bg_color = p.control;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, p.card);
    v.slider_trailing_fill = true;
    v.indent_has_left_vline = false;
    let radius = CornerRadius::same(6);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = p.control;
    v.widgets.inactive.weak_bg_fill = p.control;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.weak_bg_fill = p.accent_soft;
    v.widgets.hovered.bg_fill = p.accent_soft;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.open.weak_bg_fill = p.accent_soft;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.accent);
    v
}

/// A white (or dark) rounded card with a hairline border.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = Palette::of(ui.ctx());
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(10)
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            // Fill the column, so cards side by side line up.
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}
