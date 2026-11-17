//! Statistics and Settings.

use crate::app::{App, ConfirmState, Pending};
use crate::core::model::*;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

/* -------------------------------------------------------------- Statistics */

pub fn draw_stats(app: &mut App, ui: &mut Ui) {
    let stats = app.store.stats.clone();
    let average = if stats.runs > 0 {
        stats.lifetime_bytes / stats.runs
    } else {
        0
    };
    let best = stats.history.iter().map(|h| h.bytes).max().unwrap_or(0);

    widgets::stat_row(
        ui,
        &[
            ("Lifetime freed", &fmt_bytes(stats.lifetime_bytes), "across all clean-ups"),
            ("Files removed", &fmt_count(stats.lifetime_files), "temporary and trace files"),
            ("Clean-ups run", &fmt_count(stats.runs), "recorded on this PC"),
            ("Average per run", &fmt_bytes(average), &format!("best {}", fmt_bytes(best))),
        ],
    );

    ui.add_space(4.0);
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Recent clean-ups")
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT),
                );
                widgets::muted(ui, format!("Last {} recorded runs.", stats.history.len().min(60)));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !stats.history.is_empty()
                    && widgets::ghost_button(ui, "Reset statistics").clicked()
                {
                    app.ask(ConfirmState {
                        title: "Reset statistics?".into(),
                        intro: "Lifetime totals and the run history will be cleared. This cannot be undone."
                            .into(),
                        rows: Vec::new(),
                        actions: Vec::new(),
                        warning: None,
                        note: None,
                        confirm_text: "Reset".into(),
                        danger: true,
                        pending: Pending::ResetStats,
                    });
                }
            });
        });

        ui.add_space(6.0);
        widgets::divider(ui);
        ui.add_space(4.0);

        if stats.history.is_empty() {
            widgets::empty(ui, "Nothing recorded yet", "Run a Health Check or a clean-up and it will appear here.");
        } else {
            let max = stats.history.iter().map(|h| h.bytes).max().unwrap_or(1).max(1);
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for run in &stats.history {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(time_ago(run.at)).size(11.0).color(theme::MUTE),
                            );
                            ui.label(
                                RichText::new(widgets::truncate(&run.label, 30))
                                    .size(12.0)
                                    .color(theme::DIM),
                            );
                            widgets::bar(
                                ui,
                                run.bytes as f32 / max as f32,
                                theme::ACCENT,
                                (ui.available_width() - 190.0).max(60.0),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(fmt_bytes(run.bytes))
                                        .size(11.5)
                                        .strong()
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(format!("{} files", fmt_count(run.files)))
                                        .size(10.5)
                                        .color(theme::MUTE),
                                );
                            });
                        });
                    }
                });
        }
    });

    widgets::muted(
        ui,
        "Statistics stay on this machine in %LOCALAPPDATA%\\OpenClean\\store.json. Nothing is uploaded.",
    );
}

/* ---------------------------------------------------------------- Settings */

pub fn draw_settings(app: &mut App, ui: &mut Ui) {
    let mut changed = false;

    widgets::card(ui, |ui| {
        widgets::section(ui, "Appearance", "Pick the theme you prefer. The choice is remembered.");
        let dark = app.dark;
        if let Some(choice) = widgets::segmented(ui, &[("Dark", dark), ("Light", !dark)], "theme") {
            app.dark = choice == 0;
            app.store.settings.theme = if app.dark { "dark".into() } else { "light".into() };
            theme::apply(ui.ctx(), app.dark);
            changed = true;
        }
    });

    ui.add_space(4.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "Cleaning behaviour", "Controls that apply to every clean-up.");

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Overwrite files before deleting").size(12.5).color(theme::TEXT));
                widgets::muted(
                    ui,
                    "Writes random data over the first 8 MB of each file, then removes it.",
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut on = app.store.settings.secure_delete;
                if widgets::toggle(ui, &mut on).changed() {
                    app.store.settings.secure_delete = on;
                    changed = true;
                }
            });
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Ask before removing anything").size(12.5).color(theme::TEXT));
                widgets::muted(
                    ui,
                    "Permanent deletions always ask, regardless of this switch.",
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut on = app.store.settings.confirm_deletes;
                if widgets::toggle(ui, &mut on).changed() {
                    app.store.settings.confirm_deletes = on;
                    changed = true;
                }
            });
        });
    });

    ui.add_space(4.0);
    widgets::card(ui, |ui| {
        widgets::section(
            ui,
            "Exclusions",
            "Paths listed here are never scanned or deleted, including everything inside them.",
        );

        ui.horizontal(|ui| {
            let width = (ui.available_width() - 120.0).max(200.0);
            ui.allocate_ui(egui::vec2(width, 28.0), |ui| {
                widgets::path_input(
                    ui,
                    &mut app.exclusion_input,
                    r"C:\Users\you\AppData\Local\Something",
                );
            });
            if widgets::small_button(ui, "Add path").clicked()
                && !app.exclusion_input.trim().is_empty()
            {
                let value = app.exclusion_input.trim().to_string();
                if !app.store.settings.exclusions.contains(&value) {
                    app.store.settings.exclusions.push(value);
                    changed = true;
                }
                app.exclusion_input.clear();
            }
        });

        ui.add_space(6.0);
        let exclusions = app.store.settings.exclusions.clone();
        if exclusions.is_empty() {
            widgets::muted(ui, "No exclusions. Everything inside the cache locations is fair game.");
        }
        let mut remove: Option<String> = None;
        for path in &exclusions {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(widgets::truncate(path, 78))
                        .size(11.0)
                        .monospace()
                        .color(theme::DIM),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::small_button(ui, "Remove").clicked() {
                        remove = Some(path.clone());
                    }
                });
            });
        }
        if let Some(path) = remove {
            app.store.settings.exclusions.retain(|p| p != &path);
            changed = true;
        }
    });

    ui.add_space(4.0);
    widgets::card(ui, |ui| {
        widgets::section(ui, "About G-OpenCleaner", "Version 1.0.0 · public domain");
        widgets::key_value(ui, "Licence", "Public domain (Unlicense)");
        widgets::key_value(ui, "Activation", "None — every feature unlocked");
        widgets::key_value(ui, "Telemetry", "None — no network calls");
        widgets::key_value(ui, "Safety", "Deletions limited to disposable cache roots");
        widgets::key_value(
            ui,
            "Data folder",
            &crate::core::store::data_dir().to_string_lossy(),
        );
    });

    if changed {
        app.store.save();
    }
}
