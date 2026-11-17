//! Software Updater and Driver Updater.

use crate::app::{App, ConfirmState, Pending};
use crate::core::jobs;
use crate::core::model::*;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

/* -------------------------------------------------------- Software Updater */

pub fn draw_software(app: &mut App, ui: &mut Ui) {
    let installed = app.software.apps.len() as u64;
    let update_count = app
        .software
        .updates
        .as_ref()
        .map(|u| u.len())
        .unwrap_or(0) as u64;
    let manager = match app.software.supported {
        Some(true) => "Ready",
        Some(false) => "Missing",
        None => "—",
    };

    let updates_text = if app.software.updates.is_some() {
        fmt_count(update_count)
    } else {
        "—".to_string()
    };

    widgets::stat_row(
        ui,
        &[
            ("Installed programs", &fmt_count(installed), "registered with Windows"),
            ("Updates available", &updates_text, "via Windows Package Manager"),
            ("Package manager", manager, "winget"),
        ],
    );

    ui.add_space(4.0);
    let mut check = false;
    let mut upgrade: Option<(String, String)> = None;

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Application updates")
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT),
                );
                widgets::muted(ui, "Updates are installed with winget, built into Windows.");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if app.software.checking {
                    "Checking…"
                } else {
                    "Check for updates"
                };
                if widgets::primary_button_enabled(ui, label, !app.software.checking).clicked() {
                    check = true;
                }
            });
        });

        ui.add_space(8.0);
        widgets::divider(ui);
        ui.add_space(6.0);

        match (&app.software.supported, &app.software.updates) {
            (Some(false), _) => {
                widgets::note(
                    ui,
                    "Windows Package Manager (winget) is not available. Install App Installer from the Microsoft Store to check application updates.",
                    theme::WARN,
                );
            }
            (_, None) => {
                widgets::empty(
                    ui,
                    "Not checked yet",
                    "Press “Check for updates” to ask winget what is out of date.",
                );
            }
            (_, Some(updates)) if updates.is_empty() => {
                widgets::empty(ui, "Everything is current", "No package reported an available update.");
            }
            (_, Some(updates)) => {
                let list = updates.clone();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for item in &list {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&item.name).size(12.5).color(theme::TEXT),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · {} → {}",
                                            item.id, item.version, item.available
                                        ))
                                        .size(10.5)
                                        .monospace()
                                        .color(theme::MUTE),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if widgets::small_button(ui, "Update").clicked() {
                                            upgrade = Some((item.id.clone(), item.name.clone()));
                                        }
                                    },
                                );
                            });
                        }
                    });
            }
        }
    });

    widgets::muted(
        ui,
        "Some packages need administrator rights and will report that rather than fail silently.",
    );

    if check {
        app.software.checking = true;
        jobs::spawn_upgrades(app.tx.clone());
    }
    if let Some((id, name)) = upgrade {
        app.ask(ConfirmState {
            title: format!("Update {}?", name),
            intro: "winget will download and install the newest version. The application may close while it updates."
                .into(),
            rows: Vec::new(),
            actions: Vec::new(),
            warning: None,
            note: None,
            confirm_text: "Update now".into(),
            danger: false,
            pending: Pending::Upgrade(id, name),
        });
    }
}

/* ---------------------------------------------------------- Driver Updater */

pub fn draw_drivers(app: &mut App, ui: &mut Ui) {
    if app.drivers.loading && app.drivers.rows.is_empty() {
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                "Reading device drivers",
                "G-OpenCleaner asks Windows device management for every signed driver and its date.",
            );
            widgets::loading(ui, "Enumerating devices — this can take up to 20 seconds…");
        });
        return;
    }

    let rows = app.drivers.rows.clone();
    let old = rows.iter().filter(|d| d.old).count();
    let updates = app.drivers.updates.clone();
    let mut check_updates = false;
    let mut open_wu = false;

    let offered = if app.drivers.checking {
        "Checking…".to_string()
    } else {
        match &updates {
            Some(report) if report.error.is_none() => {
                fmt_count(report.updates.len() as u64)
            }
            _ => "—".to_string(),
        }
    };

    widgets::stat_row(
        ui,
        &[
            ("Devices", &fmt_count(rows.len() as u64), "with a signed driver"),
            ("Older than 3 years", &fmt_count(old as u64), "worth a look"),
            ("Windows Update", &offered, "drivers it offers"),
            ("Safety", "Restore point", "create one before driver work"),
        ],
    );

    ui.add_space(4.0);
    widgets::note(
        ui,
        "Driver versions come straight from Windows device management. G-OpenCleaner does not download drivers — install them from your PC or hardware maker, then re-check here.",
        theme::ACCENT,
    );

    ui.add_space(4.0);
    let mut restore = false;

    widgets::card_padded(ui, 10.0, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 560.0).max(140.0);
            ui.allocate_ui(egui::vec2(width, 28.0), |ui| {
                widgets::search(ui, &mut app.drivers.query, "Filter devices");
            });
            let mut only_old = app.drivers.only_old;
            ui.horizontal(|ui| {
                if widgets::toggle(ui, &mut only_old).changed() {
                    app.drivers.only_old = only_old;
                }
                ui.label(RichText::new("Only old drivers").size(11.5).color(theme::DIM));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if app.drivers.checking {
                    "Asking Windows Update…"
                } else {
                    "Check Windows Update"
                };
                if widgets::primary_button_enabled(ui, label, !app.drivers.checking).clicked() {
                    check_updates = true;
                }
                if widgets::small_button(ui, "Create restore point").clicked() {
                    restore = true;
                }
            });
        });

        ui.add_space(6.0);
        let query = app.drivers.query.to_lowercase();
        let visible: Vec<DriverRow> = rows
            .iter()
            .filter(|d| !app.drivers.only_old || d.old)
            .filter(|d| {
                query.is_empty()
                    || d.name.to_lowercase().contains(&query)
                    || d.class.to_lowercase().contains(&query)
                    || d.manufacturer.to_lowercase().contains(&query)
            })
            .take(200)
            .cloned()
            .collect();

        if visible.is_empty() {
            widgets::empty(ui, "No devices match", "Clear the filter to see all devices.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("driver-grid")
                        .num_columns(4)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            for header in ["Device", "Class", "Version", "Driver date"] {
                                ui.label(
                                    RichText::new(header).size(10.0).strong().color(theme::MUTE),
                                );
                            }
                            ui.end_row();

                            for driver in &visible {
                                let has_update = updates
                                    .as_ref()
                                    .map(|report| {
                                        report.updates.iter().any(|u| {
                                            driver_matches(&driver.hardware_ids, &u.hardware_id)
                                        })
                                    })
                                    .unwrap_or(false);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(widgets::truncate(&driver.name, 46))
                                            .size(12.0)
                                            .color(theme::TEXT),
                                    );
                                    if has_update {
                                        widgets::pill(ui, "update", theme::ACCENT);
                                    }
                                });
                                ui.label(
                                    RichText::new(if driver.class.is_empty() {
                                        "—".into()
                                    } else {
                                        driver.class.clone()
                                    })
                                    .size(11.0)
                                    .color(theme::DIM),
                                );
                                ui.label(
                                    RichText::new(widgets::truncate(&driver.version, 24))
                                        .size(10.5)
                                        .monospace()
                                        .color(theme::MUTE),
                                );
                                if driver.old {
                                    widgets::pill(ui, &driver.date, theme::WARN);
                                } else {
                                    ui.label(
                                        RichText::new(if driver.date.is_empty() {
                                            "—".into()
                                        } else {
                                            driver.date.clone()
                                        })
                                        .size(11.0)
                                        .color(theme::MUTE),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                });
        }
    });

    ui.add_space(5.0);
    widgets::card_padded(ui, 12.0, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Windows Update drivers")
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::small_button(ui, "Open Windows Update").clicked() {
                    open_wu = true;
                }
                if let Some(report) = &updates {
                    if report.error.is_none() {
                        ui.label(
                            RichText::new(format!(
                                "{} offered",
                                fmt_count(report.updates.len() as u64)
                            ))
                            .size(11.0)
                            .color(theme::MUTE),
                        );
                    }
                }
            });
        });
        widgets::muted(
            ui,
            "Read-only: this asks Windows Update what it offers for the devices above and matches it by hardware ID. G-OpenCleaner never downloads or installs a driver.",
        );

        ui.add_space(6.0);
        widgets::divider(ui);
        ui.add_space(4.0);

        match &updates {
            None => widgets::empty(
                ui,
                "Not checked yet",
                "Press “Check Windows Update” to ask the Windows Update service — only when you ask, never on its own.",
            ),
            Some(report) => {
                if let Some(error) = &report.error {
                    widgets::note(ui, error, theme::WARN);
                    ui.add_space(4.0);
                    if widgets::small_button(ui, "Try again").clicked() {
                        check_updates = true;
                    }
                } else if report.updates.is_empty() {
                    widgets::empty(
                        ui,
                        "Nothing offered",
                        "Windows Update publishes no newer signed driver for these devices.",
                    );
                } else {
                    for update in &report.updates {
                        let owner = rows
                            .iter()
                            .find(|r| driver_matches(&r.hardware_ids, &update.hardware_id));
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(widgets::truncate(&update.title, 76))
                                        .size(12.0)
                                        .color(theme::TEXT),
                                );
                                let mut detail = widgets::truncate(&update.hardware_id, 46);
                                if !update.date.is_empty() {
                                    if !detail.is_empty() {
                                        detail.push_str(" · ");
                                    }
                                    detail.push_str(&update.date);
                                }
                                if !detail.is_empty() {
                                    ui.label(
                                        RichText::new(detail)
                                            .size(10.5)
                                            .monospace()
                                            .color(theme::MUTE),
                                    );
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    match owner {
                                        Some(device) => {
                                            widgets::pill(ui, "newer signed driver", theme::GOOD);
                                            ui.label(
                                                RichText::new(widgets::truncate(&device.name, 36))
                                                    .size(11.0)
                                                    .color(theme::DIM),
                                            );
                                        }
                                        None => {
                                            ui.label(
                                                RichText::new("no installed device matched")
                                                    .size(10.5)
                                                    .color(theme::MUTE),
                                            );
                                        }
                                    }
                                },
                            );
                        });
                    }
                }
            }
        }
    });

    if check_updates {
        app.drivers.checking = true;
        jobs::spawn_driver_updates(app.tx.clone());
    }
    if open_wu {
        if let Err(e) = crate::core::win::open_windows_update() {
            app.toast("Could not open Windows Update", e, crate::app::ToastKind::Bad);
        }
    }

    if restore {
        app.ask(ConfirmState {
            title: "Create a system restore point?".into(),
            intro: "Windows will snapshot the current system state so you can roll back after driver or software changes. This needs administrator rights."
                .into(),
            rows: Vec::new(),
            actions: Vec::new(),
            warning: None,
            note: None,
            confirm_text: "Create restore point".into(),
            danger: false,
            pending: Pending::RestorePoint,
        });
    }
}
