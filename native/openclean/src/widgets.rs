//! Small widget kit so every screen looks like the same app.

use crate::theme;
use eframe::egui::{
    self, Align2, Color32, FontId, Frame, Margin, Pos2, Response, RichText, Rounding, Sense, Stroke,
    Ui, Vec2,
};

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_padded(ui, 16.0, add)
}

pub fn card_padded<R>(ui: &mut Ui, pad: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::none()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::LINE))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::same(pad))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(15.0).strong().color(theme::TEXT));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(12.0).color(theme::MUTE));
    }
    ui.add_space(4.0);
}

pub fn h1(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(21.0).strong().color(theme::TEXT));
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(12.0).color(theme::MUTE));
}

pub fn dim(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(12.5).color(theme::DIM));
}

pub fn strong(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(13.5).strong().color(theme::TEXT));
}

pub fn mono(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(11.5).monospace().color(theme::MUTE));
}

pub fn spacer(ui: &mut Ui, height: f32) {
    ui.add_space(height);
}

pub fn divider(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, theme::LINE);
}

/* ------------------------------------------------------------------ pills */

pub fn pill(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(10.5), color);
    let size = Vec2::new(galley.size().x + 16.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, Rounding::same(9.0), color.gamma_multiply(0.16));
    ui.painter()
        .galley(rect.min + Vec2::new(8.0, 3.0), galley, color);
}

pub fn pill_text(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).size(11.0).color(color).strong());
}

/* ------------------------------------------------------------------ stats */

pub fn stat(ui: &mut Ui, label: &str, value: &str, hint: &str) {
    Frame::none()
        .fill(theme::PANEL_2)
        .stroke(Stroke::new(1.0, theme::LINE))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::symmetric(14.0, 11.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(label.to_uppercase()).size(10.0).color(theme::MUTE).strong());
            ui.label(RichText::new(value).size(20.0).strong().color(theme::TEXT));
            if !hint.is_empty() {
                ui.label(RichText::new(hint).size(11.0).color(theme::MUTE));
            }
        });
}

pub fn stat_row(ui: &mut Ui, items: &[(&str, &str, &str)]) {
    let count = items.len().max(1);
    ui.columns(count, |cols| {
        for (i, (label, value, hint)) in items.iter().enumerate() {
            stat(&mut cols[i], label, value, hint);
        }
    });
}

/* ---------------------------------------------------------------- buttons */

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(13.0).color(Color32::WHITE))
            .fill(theme::ACCENT)
            .rounding(Rounding::same(8.0))
            .min_size(Vec2::new(0.0, 30.0)),
    )
}

pub fn primary_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(13.0).color(Color32::WHITE))
            .fill(if enabled { theme::ACCENT } else { theme::PANEL_3 })
            .rounding(Rounding::same(8.0))
            .min_size(Vec2::new(0.0, 30.0)),
    )
}

pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.5).color(theme::DIM))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, theme::LINE))
            .rounding(Rounding::same(8.0))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

pub fn small_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(theme::DIM))
            .fill(theme::PANEL_3)
            .stroke(Stroke::new(1.0, theme::LINE))
            .rounding(Rounding::same(7.0))
            .min_size(Vec2::new(0.0, 24.0)),
    )
}

pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.5).color(theme::BAD))
            .fill(theme::BAD.gamma_multiply(0.14))
            .stroke(Stroke::new(1.0, theme::BAD.gamma_multiply(0.4)))
            .rounding(Rounding::same(8.0))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

pub fn segmented(ui: &mut Ui, options: &[(&str, bool)], id: &str) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        Frame::none()
            .fill(theme::PANEL_2)
            .stroke(Stroke::new(1.0, theme::LINE))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::same(3.0))
            .show(ui, |ui| {
                for (index, (label, active)) in options.iter().enumerate() {
                    let text = RichText::new(*label)
                        .size(12.0)
                        .color(if *active { theme::TEXT } else { theme::MUTE });
                    let button = egui::Button::new(text)
                        .fill(if *active { theme::PANEL_3 } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(6.0));
                    if ui.add(button).clicked() {
                        clicked = Some(index);
                    }
                }
            });
    });
    let _ = id;
    clicked
}

/* ------------------------------------------------------------------ input */

pub fn search(ui: &mut Ui, value: &mut String, hint: &str) {
    Frame::none()
        .fill(theme::PANEL_2)
        .stroke(Stroke::new(1.0, theme::LINE))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text(hint)
                    .frame(false)
                    .desired_width(f32::INFINITY),
            );
        });
}

pub fn path_input(ui: &mut Ui, value: &mut String, hint: &str) {
    Frame::none()
        .fill(theme::PANEL_2)
        .stroke(Stroke::new(1.0, theme::LINE))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text(hint)
                    .frame(false)
                    .font(FontId::monospace(12.0))
                    .desired_width(f32::INFINITY),
            );
        });
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let bg = if *on { theme::ACCENT } else { theme::PANEL_3 };
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
    let x = if *on { rect.right() - 10.0 } else { rect.left() + 10.0 };
    ui.painter()
        .circle_filled(Pos2::new(x, rect.center().y), 7.0, Color32::WHITE);
    response
}

pub fn checkbox(ui: &mut Ui, checked: &mut bool) -> Response {
    ui.checkbox(checked, "")
}

/* --------------------------------------------------------------- visuals */

pub fn bar(ui: &mut Ui, fraction: f32, color: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(3.0), theme::PANEL_3);
    let mut filled = rect;
    filled.set_width((rect.width() * fraction.clamp(0.0, 1.0)).max(2.0));
    ui.painter().rect_filled(filled, Rounding::same(3.0), color);
}

pub fn ring(ui: &mut Ui, score: u32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let center = rect.center();
    let radius = size / 2.0 - 6.0;
    let color = theme::score_color(score);

    ui.painter()
        .circle_filled(center, radius + 6.0, theme::PANEL_2);
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(7.0, theme::PANEL_3));

    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::TAU * (score as f32 / 100.0).clamp(0.0, 1.0);
    let steps = 64;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let angle = start + sweep * (i as f32 / steps as f32);
            Pos2::new(center.x + angle.cos() * radius, center.y + angle.sin() * radius)
        })
        .collect();
    if points.len() > 1 {
        ui.painter().add(egui::Shape::line(points, Stroke::new(7.0, color)));
    }

    ui.painter().text(
        center - Vec2::new(0.0, 7.0),
        Align2::CENTER_CENTER,
        score.to_string(),
        FontId::proportional(30.0),
        theme::TEXT,
    );
    ui.painter().text(
        center + Vec2::new(0.0, 16.0),
        Align2::CENTER_CENTER,
        "HEALTH",
        FontId::proportional(9.0),
        theme::MUTE,
    );
}

/// A wide, clickable bar row used by the disk analyzer.
pub fn bar_row(ui: &mut Ui, label: &str, fraction: f32, value: &str, clickable: bool) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), if clickable { Sense::click() } else { Sense::hover() });

    if clickable && response.hovered() {
        ui.painter()
            .rect_filled(rect, Rounding::same(6.0), theme::PANEL_2);
    }

    let label_pos = Pos2::new(rect.left() + 6.0, rect.center().y);
    ui.painter().text(
        label_pos,
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        theme::DIM,
    );

    let value_width = 96.0;
    let bar_left = rect.left() + 250.0;
    let bar_right = rect.right() - value_width;
    if bar_right > bar_left {
        let bar_rect = egui::Rect::from_min_size(
            Pos2::new(bar_left, rect.center().y - 3.0),
            Vec2::new(bar_right - bar_left, 6.0),
        );
        ui.painter().rect_filled(bar_rect, Rounding::same(3.0), theme::PANEL_3);
        let mut filled = bar_rect;
        filled.set_width((bar_rect.width() * fraction.clamp(0.0, 1.0)).max(2.0));
        ui.painter()
            .rect_filled(filled, Rounding::same(3.0), theme::ACCENT);
    }

    ui.painter().text(
        Pos2::new(rect.right() - 6.0, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        FontId::proportional(11.5),
        theme::MUTE,
    );

    response
}

pub fn loading(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(14.0));
        ui.label(RichText::new(text).size(12.5).color(theme::MUTE));
    });
}

pub fn empty(ui: &mut Ui, title: &str, body: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(22.0);
        ui.label(RichText::new(title).size(13.5).color(theme::DIM));
        if !body.is_empty() {
            ui.label(RichText::new(body).size(12.0).color(theme::MUTE));
        }
        ui.add_space(22.0);
    });
}

pub fn note(ui: &mut Ui, text: &str, tone: Color32) {
    Frame::none()
        .fill(tone.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, tone.gamma_multiply(0.30)))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::symmetric(12.0, 9.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).size(12.0).color(theme::DIM));
        });
}

pub fn key_value(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(12.0).color(theme::MUTE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.0).color(theme::DIM));
        });
    });
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", head)
}
