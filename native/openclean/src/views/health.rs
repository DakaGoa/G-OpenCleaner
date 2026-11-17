//! Health Check: one score, six checks, one honestly-described fix.

use crate::app::{App, ConfirmState, Pending, View};
use crate::core::model::*;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

fn headline(score: u32) -> &'static str {
    if score >= 90 {
        "This PC is in great shape"
    } else if score >= 80 {
        "Nearly nothing to fix"
    } else if score >= 55 {
        "A few things need attention"
    } else {
        "This PC needs a clean-up"
    }
}

fn subline(report: &HealthReport) -> String {
    let junk = report.junk_bytes;
    if junk > 1024 * 1024 * 1024 {
        format!(
            "{} of junk is sitting on your drive. Every item below is a cache, log, dump or browser trace that Windows rebuilds on demand.",
            fmt_bytes(junk)
        )
    } else if junk > 0 {
        format!(
            "{} of temporary data can be cleared. These are re-creatable files only.",
            fmt_bytes(junk)
        )
    } else {
        "No junk worth removing, no privacy traces, and your drives have room to breathe.".into()
    }
}

fn check_card(app: &mut App, ui: &mut Ui, check: &HealthCheck) {
    let colour = theme::status_color(check.status);
    let mut clean_now = false;

    egui::Frame::none()
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(check.title).size(13.5).strong().color(theme::TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::pill(ui, check.status.label(), colour);
                });
            });
            ui.label(RichText::new(&check.value_text).size(18.0).strong().color(theme::TEXT));
            ui.label(RichText::new(&check.detail).size(11.5).color(theme::MUTE));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if !check.fix_rule_ids.is_empty() {
                    if widgets::small_button(ui, "Clean now").clicked() {
                        clean_now = true;
                    }
                } else if let Some(link) = check.link {
                    if widgets::small_button(ui, "Open").clicked() {
                        app.set_view(match link {
                            "startup" => View::Startup,
                            "disk" => View::Disk,
                            "performance" => View::Performance,
                            "drivers" => View::Drivers,
                            _ => View::Health,
                        });
                    }
                }
            });
        });

    if clean_now {
        let rows = vec![(check.title.to_string(), format!("{} · {}", check.value_text, check.detail))];
        app.ask(ConfirmState {
            title: format!("Clean {}?", check.title.to_lowercase()),
            intro: "These files are caches or traces and regenerate on their own.".into(),
            rows,
            actions: check.fix_actions.iter().map(|a| a.label().to_string()).collect(),
            warning: None,
            note: Some("Files currently in use are skipped, never forced.".into()),
            confirm_text: "Clean".into(),
            danger: false,
            pending: Pending::CleanRules(check.fix_rule_ids.clone(), check.title.to_string()),
        });
    }
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    let Some(report) = app.health.clone() else {
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                "Scanning this PC",
                "Measurements only — nothing is removed until you press Fix.",
            );
            widgets::loading(ui, "Reading caches, browser traces, start-up entries and drives…");
            ui.add_space(8.0);
            for line in [
                "Junk files and leftover caches",
                "Browser cookies, history and sessions",
                "Programs that launch at sign-in",
                "Drive space, memory and driver ages",
            ] {
                ui.label(
                    RichText::new(format!("·  {}", line))
                        .size(11.5)
                        .color(theme::MUTE),
                );
            }
            ui.add_space(6.0);
            widgets::muted(ui, "A full scan usually takes a few seconds.");
        });
        if !app.health_busy {
            app.load_view_data();
        }
        return;
    };

    let mut request_fix = false;

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            widgets::ring(ui, report.score, 136.0);
            ui.add_space(18.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(headline(report.score))
                        .size(19.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(RichText::new(subline(&report)).size(12.5).color(theme::MUTE));
                ui.add_space(10.0);

                ui.columns(4, |cols| {
                    widgets::stat(&mut cols[0], "Junk found", &fmt_bytes(report.junk_bytes), "");
                    widgets::stat(
                        &mut cols[1],
                        "Traces",
                        &fmt_count(report.privacy_items),
                        "",
                    );
                    widgets::stat(
                        &mut cols[2],
                        "Startup apps",
                        &report.startup_items.to_string(),
                        "",
                    );
                    widgets::stat(&mut cols[3], "Free space", &fmt_bytes(report.free_bytes), "");
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let label = if report.plan_rows.is_empty() {
                        "Nothing to fix".to_string()
                    } else {
                        format!("Fix {} issues", report.plan_rows.len())
                    };
                    if widgets::primary_button_enabled(ui, &label, !report.plan_rows.is_empty())
                        .clicked()
                    {
                        request_fix = true;
                    }
                    if widgets::ghost_button(ui, "Scan again").clicked() {
                        app.health = None;
                        app.health_busy = false;
                        app.load_view_data();
                    }
                    if report.score == 0 || report.plan_rows.is_empty() {
                        widgets::muted(ui, "Nothing found that G-OpenCleaner would remove.");
                    }
                });
            });
        });
    });

    if request_fix {
        let ids: Vec<String> = report.plan_rows.iter().map(|r| r.id.clone()).collect();
        let rows = report
            .plan_rows
            .iter()
            .take(8)
            .map(|r| {
                (
                    r.name.clone(),
                    format!("{} · {} files", fmt_bytes(r.bytes), fmt_count(r.count)),
                )
            })
            .collect();

        let permanent: Vec<String> = report
            .excluded
            .iter()
            .filter(|name| {
                app.rules
                    .iter()
                    .any(|r| &r.name == *name && r.permanent)
            })
            .cloned()
            .collect();

        let warning = if permanent.is_empty() {
            None
        } else {
            Some(format!(
                "{} is not emptied here. That permanently destroys files you can still restore, so it stays a deliberate choice in Custom Clean.",
                permanent.join(", ")
            ))
        };

        app.ask(ConfirmState {
            title: "Fix everything found?".into(),
            intro: format!(
                "G-OpenCleaner will permanently remove {} across {} files:",
                fmt_bytes(report.plan_bytes),
                fmt_count(report.plan_count)
            ),
            rows,
            actions: report
                .plan_actions
                .iter()
                .map(|a| a.label().to_string())
                .collect(),
            warning,
            note: Some(
                "Every item above is a cache, log, memory dump or browser trace that Windows and your apps rebuild. No documents, photos, saved passwords or installed programs are touched."
                    .into(),
            ),
            confirm_text: "Clean now".into(),
            danger: false,
            pending: Pending::CleanRules(ids, "Health Check".into()),
        });
    }

    ui.add_space(4.0);
    let checks = report.checks.clone();
    for pair in checks.chunks(2) {
        ui.columns(2, |cols| {
            for (i, check) in pair.iter().enumerate() {
                check_card(app, &mut cols[i], check);
            }
        });
        ui.add_space(2.0);
    }

    ui.add_space(6.0);
    widgets::muted(
        ui,
        "Health Check reads file sizes and system counters only. Nothing is removed until you press Fix.",
    );
}
