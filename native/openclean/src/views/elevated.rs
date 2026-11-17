//! Administrator Tasks: the cleanups that need an elevated copy of the app.

use crate::app::{App, ConfirmState, Pending};
use crate::core::jobs;
use crate::core::model::*;
use crate::core::elevate;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

/// The component-store action label. A measured store with nothing spare is not
/// the same thing as a store DISM would not report a size for.
pub fn reclaim_label(measured: bool, reclaimable: Option<u64>) -> &'static str {
    match (measured, reclaimable) {
        (false, _) => "Measure first",
        (true, Some(0)) => "Nothing to reclaim",
        (true, Some(_)) => "Run DISM cleanup",
        (true, None) => "No size to act on",
    }
}

fn row(ui: &mut Ui, label: &str, value: &str, dim: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(11.5)
                .color(if dim { theme::MUTE } else { theme::DIM }),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(11.5).color(theme::MUTE));
        });
    });
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    let measured = app.elevated.measured.clone();
    let result = app.elevated.result.clone();
    let busy = app.elevated.busy.clone();
    let mut restore = app.elevated.restore_point;

    let mut measure_now = false;
    let mut run: Option<&'static str> = None;

    let old_bytes = measured.as_ref().and_then(|r| r.value("windows_old"));
    let store_bytes = measured
        .as_ref()
        .and_then(|r| r.value("store_reclaimable"));
    // A failed measurement is not a measurement of zero.
    let was_measured = matches!(&measured, Some(r) if r.ok);

    let old_label = match (was_measured, old_bytes) {
        (_, Some(bytes)) => fmt_bytes(bytes),
        (false, None) => "Not measured".to_string(),
        (true, None) => "Not on this PC".to_string(),
    };
    let store_actual = measured
        .as_ref()
        .and_then(|r| r.value("store_actual"));
    let store_label = match (was_measured, store_bytes) {
        (_, Some(0)) => "Nothing to reclaim".to_string(),
        (_, Some(bytes)) => fmt_bytes(bytes),
        (false, None) => "Not measured".to_string(),
        (true, None) => "No size reported".to_string(),
    };
    let old_hint = if was_measured && old_bytes.is_none() {
        "not kept on this PC"
    } else {
        "previous installation"
    };
    let store_hint = match (was_measured, store_bytes) {
        (false, _) => "measure to find out",
        (true, Some(0)) => "nothing spare to remove",
        (true, Some(_)) => "reclaimable now",
        (true, None) => "DISM gave no figure",
    };

    widgets::stat_row(
        ui,
        &[
            ("Windows.old", old_label.as_str(), old_hint),
            ("Component store", store_label.as_str(), store_hint),
            ("Approval", "Per task", "Windows UAC prompt"),
        ],
    );

    ui.add_space(4.0);
    widgets::note(
        ui,
        "These cleanups touch parts of Windows that belong to an administrator. Pressing one raises the Windows approval prompt, and only then does a second copy of G-OpenCleaner do the work — this window never runs elevated, and nothing is changed if you dismiss the prompt.",
        theme::ACCENT,
    );
    ui.add_space(5.0);

    widgets::card_padded(ui, 12.0, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("What is reclaimable?")
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT),
                );
                widgets::muted(
                    ui,
                    "Windows will not report the size of either area to a standard user. Measuring asks for approval once and changes nothing.",
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let waiting = busy.as_deref() == Some(elevate::TASK_MEASURE);
                let label = if waiting {
                    "Waiting for Windows…"
                } else {
                    "Measure"
                };
                if busy.is_some() {
                    ui.add(egui::Spinner::new().size(14.0));
                }
                if widgets::small_button(ui, label).clicked() && busy.is_none() {
                    measure_now = true;
                }
            });
        });

        if let Some(m) = &measured {
            ui.add_space(6.0);
            widgets::divider(ui);
            for (label, value) in &m.rows {
                row(ui, label, value, false);
            }
        }
    });

    ui.add_space(5.0);

    ui.add_space(6.0);
    widgets::card_padded(ui, 12.0, |ui| {
        /* ------------------------------------------------------ Windows.old */
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Previous Windows installation")
                        .size(13.5)
                        .strong()
                        .color(theme::TEXT),
                );
                widgets::mono(
                    ui,
                    widgets::truncate(&elevate::windows_old_path().display().to_string(), 70),
                );
                widgets::muted(
                    ui,
                    "The old build Windows keeps so you could roll back after an upgrade. It is owned by TrustedInstaller, which is why a standard user cannot remove it.",
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_run = old_bytes.map(|b| b > 0).unwrap_or(false) && busy.is_none();
                let label = match (was_measured, old_bytes) {
                    (false, _) => "Measure first".to_string(),
                    (true, None) => "Not on this PC".to_string(),
                    (true, Some(bytes)) => format!("Delete {}", fmt_bytes(bytes)),
                };
                if can_run {
                    if widgets::danger_button(ui, &label).clicked() {
                        run = Some(elevate::TASK_WINDOWS_OLD);
                    }
                } else {
                    ui.add_enabled(
                        false,
                        egui::Button::new(RichText::new(label).size(12.0).color(theme::MUTE)),
                    );
                }
            });
        });

        ui.add_space(8.0);
        widgets::divider(ui);
        ui.add_space(8.0);

        /* ------------------------------------------------ component store */
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Component store cleanup")
                        .size(13.5)
                        .strong()
                        .color(theme::TEXT),
                );
                widgets::mono(
                    ui,
                    format!(r"{}\WinSxS · DISM StartComponentCleanup", crate::core::win::windows_dir()),
                );
                widgets::muted(
                    ui,
                    "Removes superseded update components and their backups. Updates already installed stay installed.",
                );
                if was_measured && store_bytes == Some(0) {
                    widgets::muted(
                        ui,
                        format!(
                            "DISM measured the store at {} and reports no backups or cache left to reclaim, so there is nothing to remove.",
                            store_actual
                                .map(fmt_bytes)
                                .unwrap_or_else(|| "its current size".into())
                        ),
                    );
                } else if was_measured && store_bytes.is_none() {
                    widgets::muted(
                        ui,
                        "DISM reported no size for the component store at all, so there is no figure to confirm against. Measure again once Windows has finished its own maintenance.",
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_run =
                    store_bytes.map(|bytes| bytes > 0).unwrap_or(false) && busy.is_none();
                let label = reclaim_label(was_measured, store_bytes).to_string();
                if can_run {
                    if widgets::danger_button(ui, &label).clicked() {
                        run = Some(elevate::TASK_COMPONENT_STORE);
                    }
                } else {
                    ui.add_enabled(
                        false,
                        egui::Button::new(RichText::new(label).size(12.0).color(theme::MUTE)),
                    );
                }
            });
        });

        ui.add_space(8.0);
        widgets::divider(ui);
        ui.add_space(8.0);

        ui.checkbox(
            &mut restore,
            "Create a restore point before these tasks",
        );
        widgets::muted(
            ui,
            "Needs System Protection. If it is off, the task stops before changing anything and tells you, so uncheck this only if you mean to go ahead without one.",
        );
    });

    if let Some(task) = &busy {
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0));
            widgets::muted(
                ui,
                if task == elevate::TASK_MEASURE {
                    "Waiting for the Windows approval prompt. Nothing is changed by measuring."
                } else {
                    "Waiting for approval, then working. Windows can take several minutes over a large Windows.old."
                },
            );
        });
    }

    if let Some(r) = &result {
        ui.add_space(5.0);
        widgets::card_padded(ui, 12.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if r.ok {
                        "Last administrator task"
                    } else {
                        "Task stopped"
                    })
                    .size(13.0)
                    .strong()
                    .color(if r.ok { theme::TEXT } else { theme::WARN }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if r.freed > 0 {
                        ui.label(
                            RichText::new(format!("{} freed", fmt_bytes(r.freed)))
                                .size(11.5)
                                .color(theme::GOOD),
                        );
                    }
                });
            });
            ui.label(RichText::new(&r.message).size(11.5).color(theme::DIM));
            for (label, value) in &r.rows {
                row(ui, label, value, true);
            }
        });
    }

    widgets::muted(
        ui,
        "G-OpenCleaner never runs these two tasks without the Windows prompt, and the helper refuses any path other than the one Windows.old folder.",
    );

    app.elevated.restore_point = restore;

    if measure_now {
        app.elevated.busy = Some(elevate::TASK_MEASURE.to_string());
        app.elevated.result = None;
        jobs::spawn_elevated(elevate::TASK_MEASURE.to_string(), false, app.tx.clone());
    }

    if let Some(task) = run {
        let size_row = if task == elevate::TASK_WINDOWS_OLD {
            ("Size".to_string(), old_label.clone())
        } else {
            (
                "Reclaimable".to_string(),
                store_label.clone(),
            )
        };
        let rows = vec![
            size_row,
            (
                "Restore point".to_string(),
                if restore {
                    "Create first".to_string()
                } else {
                    "Not requested".to_string()
                },
            ),
        ];
        let note = if restore {
            "A restore point is created before anything changes. If System Protection is off the task stops there."
        } else {
            "No restore point will be created."
        };
        app.ask(ConfirmState {
            title: if task == elevate::TASK_WINDOWS_OLD {
                "Delete Windows.old?".into()
            } else {
                "Clean up the component store?".into()
            },
            intro: if task == elevate::TASK_WINDOWS_OLD {
                format!(
                    "{} will be removed from {}. Windows will ask for administrator approval next.",
                    old_label,
                    elevate::windows_old_path().display()
                )
            } else {
                format!(
                    "DISM will remove superseded update components and their backups, about {} reclaimable.",
                    store_label
                )
            },
            rows,
            actions: Vec::new(),
            warning: Some(if task == elevate::TASK_WINDOWS_OLD {
                "You will not be able to roll back to the previous Windows build afterwards. This cannot be undone."
                    .into()
            } else {
                "DISM is asked to run StartComponentCleanup with ResetBase, so updates that are already installed can no longer be uninstalled."
                    .into()
            }),
            note: Some(note.into()),
            confirm_text: if task == elevate::TASK_WINDOWS_OLD {
                format!("Delete {}", old_label)
            } else {
                "Run cleanup".into()
            },
            danger: true,
            pending: Pending::ElevatedTask(task.to_string()),
        });
    }
}
