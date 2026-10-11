//! Look and feel: dark first, near-black surfaces with one cyan accent (and
//! a light variant), IBM Plex Sans for text and Orbitron for readings.

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
    /// The navigation sidebar and title bar.
    pub sidebar: Color32,
    /// Hovered and selected rows.
    pub raised: Color32,
}

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf2, 0xf3, 0xf5),
    card: Color32::from_rgb(0xff, 0xff, 0xff),
    border: Color32::from_rgb(0xe2, 0xe4, 0xe8),
    text: Color32::from_rgb(0x15, 0x17, 0x1c),
    muted: Color32::from_rgb(0x64, 0x69, 0x73),
    faint: Color32::from_rgb(0x9a, 0x9f, 0xa8),
    accent: Color32::from_rgb(0x08, 0x9b, 0xb0),
    accent_soft: Color32::from_rgb(0xe2, 0xf4, 0xf7),
    track: Color32::from_rgb(0xe8, 0xea, 0xee),
    control: Color32::from_rgb(0xf4, 0xf5, 0xf7),
    good: Color32::from_rgb(0x16, 0xa3, 0x6a),
    warn: Color32::from_rgb(0xd9, 0x8a, 0x06),
    bad: Color32::from_rgb(0xdc, 0x35, 0x3c),
    bezel: Color32::from_rgb(0x1c, 0x1e, 0x22),
    sidebar: Color32::from_rgb(0xfa, 0xfa, 0xfb),
    raised: Color32::from_rgb(0xee, 0xf0, 0xf3),
};

/// Near-black surfaces, hairline borders and one cyan accent, matching the
/// cooler-screen skins.
pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x0b, 0x0c, 0x0f),
    card: Color32::from_rgb(0x13, 0x15, 0x19),
    border: Color32::from_rgb(0x22, 0x25, 0x2b),
    text: Color32::from_rgb(0xec, 0xed, 0xf0),
    muted: Color32::from_rgb(0x8b, 0x90, 0x99),
    faint: Color32::from_rgb(0x5c, 0x61, 0x6a),
    accent: Color32::from_rgb(0x2e, 0xd3, 0xe6),
    accent_soft: Color32::from_rgb(0x10, 0x2a, 0x2f),
    track: Color32::from_rgb(0x23, 0x26, 0x2c),
    control: Color32::from_rgb(0x1a, 0x1c, 0x21),
    good: Color32::from_rgb(0x3d, 0xdc, 0x97),
    warn: Color32::from_rgb(0xf5, 0xb8, 0x41),
    bad: Color32::from_rgb(0xf2, 0x55, 0x5a),
    bezel: Color32::from_rgb(0x05, 0x05, 0x06),
    sidebar: Color32::from_rgb(0x0e, 0x0f, 0x12),
    raised: Color32::from_rgb(0x1b, 0x1e, 0x23),
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
const NUMERIC: &str = "numeric";

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Orbitron, the cooler screen's face, for big readings.
pub fn numeric(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(NUMERIC.into()))
}

/// Installs fonts, text sizes and both light and dark visuals.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 4] = [
        (
            NUMERIC,
            include_bytes!("../../../assets/fonts/Orbitron.ttf"),
        ),
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
    fonts
        .families
        .insert(FontFamily::Name(NUMERIC.into()), family(NUMERIC));
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
            // An app, not a document: labels shouldn't be text-selectable.
            style.interaction.selectable_labels = false;
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
    v.handle_shape = egui::style::HandleShape::Circle;
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
    v.widgets.hovered.weak_bg_fill = p.raised;
    v.widgets.hovered.bg_fill = p.raised;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.open.weak_bg_fill = p.raised;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.border);
    v
}

/// A white (or dark) rounded card with a hairline border.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = Palette::of(ui.ctx());
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            // Fill the column, so cards side by side line up.
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}
