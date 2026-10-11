//! The app's pages, plus layout pieces they share.

pub mod dashboard;
pub mod device;
pub mod devices;
pub mod settings;

use eframe::egui;

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};

/// Page title with an optional muted subtitle.
pub fn header(ui: &mut egui::Ui, title: &str, subtitle: Option<&str>) {
    let p = Palette::of(ui.ctx());
    ui.label(
        egui::RichText::new(title)
            .font(theme::semibold(24.0))
            .color(p.text),
    );
    if let Some(subtitle) = subtitle {
        ui.label(egui::RichText::new(subtitle).color(p.muted));
    }
    ui.add_space(14.0);
}

/// Card heading: icon and title on the left, muted detail on the right.
pub fn card_title(ui: &mut egui::Ui, icon: Icon, title: &str, detail: Option<&str>) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        // Claude's mark keeps its own color.
        let color = if icon == Icon::Claude {
            icons::CLAUDE
        } else {
            p.text
        };
        icons::show(ui, icon, 18.0, color);
        ui.label(
            egui::RichText::new(title)
                .font(theme::semibold(16.0))
                .color(p.text),
        );
        if let Some(detail) = detail {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(detail).color(p.muted)).truncate());
            });
        }
    });
    ui.add_space(10.0);
}

/// A settings line: title and description on the left, a control on the right.
pub fn setting_row(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    let p = Palette::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                egui::RichText::new(title)
                    .font(theme::medium(15.0))
                    .color(p.text),
            );
            ui.add(egui::Label::new(egui::RichText::new(description).color(p.muted)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}
