//! Colour palette and egui style. One accent, one radius scale everywhere.

use eframe::egui::{self, Color32, Rounding, Stroke};

pub const BG: Color32 = Color32::from_rgb(0x0a, 0x0d, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x17, 0x22);
pub const PANEL_2: Color32 = Color32::from_rgb(0x16, 0x1c, 0x28);
pub const PANEL_3: Color32 = Color32::from_rgb(0x1b, 0x22, 0x30);
pub const LINE: Color32 = Color32::from_rgb(0x25, 0x2e, 0x40);
pub const TEXT: Color32 = Color32::from_rgb(0xe9, 0xee, 0xf8);
pub const DIM: Color32 = Color32::from_rgb(0xa3, 0xb0, 0xc6);
pub const MUTE: Color32 = Color32::from_rgb(0x78, 0x85, 0x9c);
pub const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x8c, 0xff);
pub const ACCENT_2: Color32 = Color32::from_rgb(0x8b, 0x5c, 0xf6);
pub const GOOD: Color32 = Color32::from_rgb(0x2d, 0xd4, 0xa7);
pub const WARN: Color32 = Color32::from_rgb(0xf5, 0xb5, 0x44);
pub const BAD: Color32 = Color32::from_rgb(0xff, 0x5f, 0x6d);

pub const RADIUS: f32 = 10.0;

// Light theme values, kept deliberately close to the web build.
pub const LIGHT_BG: Color32 = Color32::from_rgb(0xf3, 0xf5, 0xf9);
pub const LIGHT_PANEL: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const LIGHT_PANEL_2: Color32 = Color32::from_rgb(0xf7, 0xf9, 0xfc);
pub const LIGHT_PANEL_3: Color32 = Color32::from_rgb(0xee, 0xf2, 0xf9);
pub const LIGHT_LINE: Color32 = Color32::from_rgb(0xdd, 0xe3, 0xec);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(0x13, 0x1a, 0x26);
pub const LIGHT_DIM: Color32 = Color32::from_rgb(0x4c, 0x5a, 0x70);
pub const LIGHT_MUTE: Color32 = Color32::from_rgb(0x71, 0x80, 0x9a);

pub fn apply(ctx: &egui::Context, dark: bool) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };

    let panel = if dark { PANEL } else { LIGHT_PANEL };
    let panel_2 = if dark { PANEL_2 } else { LIGHT_PANEL_2 };
    let panel_3 = if dark { PANEL_3 } else { LIGHT_PANEL_3 };
    let line = if dark { LINE } else { LIGHT_LINE };
    let text = if dark { TEXT } else { LIGHT_TEXT };
    let dim = if dark { DIM } else { LIGHT_DIM };

    visuals.dark_mode = dark;
    visuals.panel_fill = if dark { BG } else { LIGHT_BG };
    visuals.window_fill = panel_2;
    visuals.extreme_bg_color = panel_3;
    visuals.faint_bg_color = panel_2;
    visuals.override_text_color = Some(text);
    visuals.window_rounding = Rounding::same(12.0);
    visuals.menu_rounding = Rounding::same(10.0);
    visuals.window_stroke = Stroke::new(1.0, line);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;

    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.weak_bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, dim);
    visuals.widgets.noninteractive.rounding = Rounding::same(RADIUS);

    visuals.widgets.inactive.bg_fill = panel_3;
    visuals.widgets.inactive.weak_bg_fill = panel_3;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    visuals.widgets.inactive.rounding = Rounding::same(8.0);

    visuals.widgets.hovered.bg_fill = panel_3;
    visuals.widgets.hovered.weak_bg_fill = panel_3;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text);
    visuals.widgets.hovered.rounding = Rounding::same(8.0);

    visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.85);
    visuals.widgets.active.weak_bg_fill = ACCENT.gamma_multiply(0.85);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.rounding = Rounding::same(8.0);

    visuals.widgets.open.bg_fill = panel_2;
    visuals.widgets.open.weak_bg_fill = panel_2;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, line);
    visuals.widgets.open.rounding = Rounding::same(8.0);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    style.text_styles = [
        (egui::TextStyle::Small, egui::FontId::proportional(11.5)),
        (egui::TextStyle::Body, egui::FontId::proportional(13.5)),
        (egui::TextStyle::Button, egui::FontId::proportional(13.0)),
        (egui::TextStyle::Heading, egui::FontId::proportional(19.0)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(12.0)),
    ]
    .into();

    ctx.set_style(style);
}

pub fn status_color(status: crate::core::model::Status) -> Color32 {
    use crate::core::model::Status;
    match status {
        Status::Good => GOOD,
        Status::Warn => WARN,
        Status::Bad => BAD,
    }
}

pub fn score_color(score: u32) -> Color32 {
    if score >= 80 {
        GOOD
    } else if score >= 55 {
        WARN
    } else {
        BAD
    }
}
