//! Quarantine: everything the cleaner removed recently, ready to put back.

use crate::app::{App, ConfirmState, Pending};
use crate::core::jobs;
use crate::core::model::*;
use crate::core::quarantine;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

pub fn draw(app: &mut App, ui: &mut Ui) {
    let runs = app.quarantine.runs.clone();
    let bytes: u64 = runs.iter().map(|r| r.bytes).sum();
    let files: u64 = runs.iter().map(|r| r.entries.len() as u64).sum();

    widgets::stat_row(
        ui,
        &[
            (
                "Held in quarantine",
                &fmt_bytes(bytes),
                "space returns after the purge",
            ),
            ("Files", &fmt_count(files), "each can be put back"),
            (
                "Retention",
                &format!("{} days", quarantine::RETENTION_DAYS),
                "then deleted automatically",
            ),
        ],
    );

    ui.add_space(4.0);
    widgets::note(
        ui,
        "Removing a cache file moves it here instead of destroying it. Only the Recycle Bin and files you pick yourself in the analyzers are deleted outright.",
        theme::ACCENT,
    );
    ui.add_space(4.0);

    let mut restore: Option<(String, String, String)> = None;
    let mut purge = false;
    let mut refresh = false;

    widgets::card_padded(ui, 12.0, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Quarantined files")
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT),
                );
                widgets::muted(ui, format!("{} runs · {} files", runs.len(), files));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if files > 0 && widgets::danger_button(ui, "Purge everything").clicked() {
                    purge = true;
                }
                if widgets::small_button(ui, "Refresh").clicked() {
                    refresh = true;
                }
            });
        });

        ui.add_space(6.0);
        widgets::divider(ui);

        if app.quarantine.loading {
            ui.add_space(6.0);
            widgets::loading(ui, "Reading the quarantine folder…");
            return;
        }

        if runs.is_empty() {
            widgets::empty(
                ui,
                "Nothing in quarantine",
                "Clean something and its files will wait here for seven days.",
            );
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(400.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for run in &runs {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(widgets::truncate(&run.label, 42))
                                .size(12.5)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.label(
                            RichText::new(time_ago(run.at))
                                .size(10.5)
                                .color(theme::MUTE),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} files",
                                        fmt_bytes(run.bytes),
                                        run.entries.len()
                                    ))
                                    .size(11.0)
                                    .color(theme::MUTE),
                                );
                            },
                        );
                    });

                    for entry in &run.entries {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(widgets::truncate(&entry.original, 70))
                                    .size(10.5)
                                    .monospace()
                                    .color(theme::DIM),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::small_button(ui, "Restore").clicked() {
                                        restore = Some((
                                            run.id.clone(),
                                            entry.stored.clone(),
                                            entry.original.clone(),
                                        ));
                                    }
                                    ui.add_space(10.0);
                                    ui.label(
                                        RichText::new(fmt_bytes(entry.size))
                                            .size(10.5)
                                            .color(theme::MUTE),
                                    );
                                },
                            );
                        });
                    }

                    ui.add_space(6.0);
                    widgets::divider(ui);
                }
            });
    });

    widgets::muted(
        ui,
        "Restoring puts a file back where it came from. Purging frees the space and cannot be undone.",
    );

    if refresh {
        app.quarantine.loading = true;
        jobs::spawn_quarantine_list(app.tx.clone());
    }

    if let Some((run_id, stored, original)) = restore {
        app.ask(ConfirmState {
            title: "Restore this file?".into(),
            intro: original.clone(),
            rows: Vec::new(),
            actions: Vec::new(),
            warning: None,
            note: Some(
                "It goes back to exactly that path. If a file already exists there, the restore is refused."
                    .into(),
            ),
            confirm_text: "Restore".into(),
            danger: false,
            pending: Pending::RestoreEntry(run_id, stored, original),
        });
    }

    if purge {
        app.ask(ConfirmState {
            title: "Delete everything in quarantine?".into(),
            intro: format!(
                "{} across {} files will be destroyed now, without waiting for the {} days.",
                fmt_bytes(bytes),
                fmt_count(files),
                quarantine::RETENTION_DAYS
            ),
            rows: Vec::new(),
            actions: Vec::new(),
            warning: Some(
                "This cannot be undone. Nothing here can be restored afterwards.".into(),
            ),
            note: None,
            confirm_text: "Purge permanently".into(),
            danger: true,
            pending: Pending::PurgeQuarantine,
        });
    }
}
