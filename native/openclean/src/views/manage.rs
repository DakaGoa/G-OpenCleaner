//! Startup Manager, Performance Optimizer and Uninstaller.

use crate::app::{App, ConfirmState, Pending, ToastKind};
use crate::core::jobs;
use crate::core::model::*;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

/* --------------------------------------------------------- Startup Manager */

pub fn draw_startup(app: &mut App, ui: &mut Ui) {
    if app.startup.loading && app.startup.items.is_empty() {
        widgets::card(ui, |ui| widgets::loading(ui, "Reading startup entries…"));
        return;
    }

    let items = app.startup.items.clone();
    let enabled = items.iter().filter(|i| i.enabled).count();

    widgets::stat_row(
        ui,
        &[
            ("Startup apps", &items.len().to_string(), "registered entries"),
            (
                "Running at sign-in",
                &enabled.to_string(),
                if enabled > 6 { "this slows boot" } else { "healthy range" },
            ),
            (
                "Disabled",
                &(items.len().saturating_sub(enabled)).to_string(),
                "kept, easy to restore",
            ),
        ],
    );

    ui.add_space(4.0);
    if let Some(system) = &app.system {
        if !system.admin {
            widgets::note(
                ui,
                "Standard user rights. Entries under HKLM (all users) cannot be changed without running G-OpenCleaner as administrator.",
                theme::WARN,
            );
            ui.add_space(4.0);
        }
    }

    let mut toggle: Option<(StartupItem, bool)> = None;
    let mut remove: Option<StartupItem> = None;

    widgets::card_padded(ui, 10.0, |ui| {
        ui.horizontal(|ui| {
            let active = app.startup.filter;
            if let Some(choice) = widgets::segmented(
                ui,
                &[
                    ("All", active == 0),
                    ("Enabled", active == 1),
                    ("Disabled", active == 2),
                ],
                "startup-filter",
            ) {
                app.startup.filter = choice as u8;
            }
            let width = (ui.available_width() - 220.0).max(160.0);
            ui.allocate_ui(egui::vec2(width, 28.0), |ui| {
                widgets::search(ui, &mut app.startup.query, "Filter by name or command");
            });
        });

        ui.add_space(6.0);
        let query = app.startup.query.to_lowercase();
        let visible: Vec<StartupItem> = items
            .iter()
            .filter(|item| match app.startup.filter {
                1 => item.enabled,
                2 => !item.enabled,
                _ => true,
            })
            .filter(|item| {
                query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.command.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();

        if visible.is_empty() {
            widgets::empty(ui, "Nothing to show", "No startup entries match this filter.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for item in &visible {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if item.kind == StartupKind::Registry {
                                let mut on = item.enabled;
                                if widgets::toggle(ui, &mut on).changed() {
                                    toggle = Some((item.clone(), on));
                                }
                            } else {
                                widgets::pill(ui, "shortcut", theme::MUTE);
                            }

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&item.name)
                                            .size(12.5)
                                            .color(theme::TEXT),
                                    );
                                    if item.enabled {
                                        widgets::pill(ui, "enabled", theme::GOOD);
                                    } else {
                                        widgets::pill(ui, "disabled", theme::MUTE);
                                    }
                                    ui.label(
                                        RichText::new(&item.source).size(10.0).color(theme::MUTE),
                                    );
                                });
                                ui.label(
                                    RichText::new(widgets::truncate(&item.command, 72))
                                        .size(10.5)
                                        .monospace()
                                        .color(theme::MUTE),
                                )
                                .on_hover_text(&item.command);
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::small_button(ui, "Remove").clicked() {
                                        remove = Some(item.clone());
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);
                        widgets::divider(ui);
                    }
                });
        }
    });

    widgets::muted(
        ui,
        "Disabling moves the entry into a backup key instead of deleting it, so nothing is lost.",
    );

    if let Some((item, on)) = toggle {
        jobs::spawn_startup_toggle(item, on, app.tx.clone());
    }
    if let Some(item) = remove {
        app.ask(ConfirmState {
            title: format!("Remove {}?", item.name),
            intro: "This deletes the startup entry. The program stays installed, it simply stops launching on its own."
                .into(),
            rows: vec![(item.command.clone(), String::new())],
            actions: Vec::new(),
            warning: None,
            note: None,
            confirm_text: "Remove entry".into(),
            danger: true,
            pending: Pending::RemoveStartup(item),
        });
    }
}

/* ---------------------------------------------------- Performance Optimizer */

pub fn draw_performance(app: &mut App, ui: &mut Ui) {
    let total_mem = app.system.as_ref().map(|s| s.mem_total).unwrap_or(0);
    let free_mem = app.system.as_ref().map(|s| s.mem_free).unwrap_or(0);
    let used_pct = if total_mem > 0 {
        (total_mem.saturating_sub(free_mem)) as f32 / total_mem as f32 * 100.0
    } else {
        0.0
    };

    widgets::stat_row(
        ui,
        &[
            ("Processes", &fmt_count(app.perf.procs.len() as u64), "running now"),
            (
                "Memory in use",
                &format!("{:.0}%", used_pct),
                &format!(
                    "{} of {}",
                    fmt_bytes(total_mem.saturating_sub(free_mem)),
                    fmt_bytes(total_mem)
                ),
            ),
            (
                "Uptime",
                &fmt_uptime(app.system.as_ref().map(|s| s.uptime_secs).unwrap_or(0)),
                "since last reboot",
            ),
        ],
    );

    ui.add_space(4.0);
    let mut kill: Option<(u32, String)> = None;
    let mut refresh = false;

    widgets::card_padded(ui, 10.0, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 220.0).max(160.0);
            ui.allocate_ui(egui::vec2(width, 28.0), |ui| {
                widgets::search(ui, &mut app.perf.query, "Filter processes");
            });
            if widgets::small_button(ui, "Refresh").clicked() {
                refresh = true;
            }
            if app.perf.loading {
                ui.add(egui::Spinner::new().size(14.0));
            }
        });

        ui.add_space(6.0);
        let query = app.perf.query.to_lowercase();
        let procs: Vec<ProcRow> = app
            .perf
            .procs
            .iter()
            .filter(|p| query.is_empty() || p.name.to_lowercase().contains(&query))
            .take(80)
            .cloned()
            .collect();

        if procs.is_empty() {
            if app.perf.loading {
                widgets::loading(ui, "Sampling running processes…");
            } else {
                widgets::empty(ui, "No processes match", "");
            }
        } else {
            egui::ScrollArea::vertical()
                .max_height(312.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for proc in &procs {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(widgets::truncate(&proc.name, 44))
                                    .size(12.0)
                                    .color(theme::TEXT),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if proc.pid == std::process::id() {
                                        widgets::pill(ui, "this app", theme::MUTE);
                                    } else if crate::core::win::is_critical_process(&proc.name) {
                                        widgets::pill(ui, "protected", theme::MUTE);
                                    } else if widgets::small_button(ui, "End").clicked() {
                                        kill = Some((proc.pid, proc.name.clone()));
                                    }
                                    ui.add_space(14.0);
                                    ui.label(
                                        RichText::new(format!("PID {}", proc.pid))
                                            .size(10.5)
                                            .color(theme::MUTE),
                                    );
                                    ui.add_space(14.0);
                                    ui.label(
                                        RichText::new(fmt_bytes(proc.memory))
                                            .size(11.0)
                                            .color(theme::MUTE),
                                    );
                                    ui.add_space(14.0);
                                    ui.label(
                                        RichText::new(format!("{:.1}%", proc.cpu_pct))
                                            .size(11.0)
                                            .color(if proc.cpu_pct > 20.0 {
                                                theme::WARN
                                            } else {
                                                theme::MUTE
                                            }),
                                    );
                                },
                            );
                        });
                        ui.add_space(3.0);
                    }
                });
        }
    });

    if !app.perf.tasks.is_empty() {
        ui.add_space(4.0);
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                "Scheduled tasks",
                "Non-Microsoft tasks registered on this PC.",
            );
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for task in app.perf.tasks.iter().take(40) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&task.name).size(12.0).color(theme::DIM));
                            if !task.path.trim().is_empty() && task.path.trim() != "\\" {
                                ui.label(
                                    RichText::new(widgets::truncate(&task.path, 40))
                                        .size(10.5)
                                        .monospace()
                                        .color(theme::MUTE),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let colour = if task.state == "Ready" {
                                        theme::GOOD
                                    } else {
                                        theme::MUTE
                                    };
                                    widgets::pill(ui, &task.state, colour);
                                },
                            );
                        });
                    }
                });
        });
    }

    if refresh {
        app.perf.loading = true;
        jobs::spawn_processes(app.tx.clone());
    }

    if let Some((pid, name)) = kill {
        app.ask(ConfirmState {
            title: format!("End {}?", name),
            intro: "Unsaved work in this process will be lost. Ending system processes can make the desktop unstable."
                .into(),
            rows: Vec::new(),
            actions: Vec::new(),
            warning: None,
            note: None,
            confirm_text: "End process".into(),
            danger: true,
            pending: Pending::Kill(pid, name),
        });
    }
}

/* ------------------------------------------------------------- Uninstaller */

pub fn draw_uninstaller(app: &mut App, ui: &mut Ui) {
    if app.uninstall.loading && app.uninstall.apps.is_empty() {
        widgets::card(ui, |ui| widgets::loading(ui, "Reading installed programs…"));
        return;
    }

    let apps = app.uninstall.apps.clone();
    let mut launch: Option<AppEntry> = None;

    widgets::card_padded(ui, 10.0, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 180.0).max(180.0);
            ui.allocate_ui(egui::vec2(width, 28.0), |ui| {
                widgets::search(ui, &mut app.uninstall.query, "Search installed programs");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::muted(ui, format!("{} programs", fmt_count(apps.len() as u64)));
            });
        });

        ui.add_space(6.0);
        let query = app.uninstall.query.to_lowercase();
        let visible: Vec<AppEntry> = apps
            .iter()
            .filter(|a| {
                query.is_empty()
                    || a.name.to_lowercase().contains(&query)
                    || a.publisher.to_lowercase().contains(&query)
            })
            .take(300)
            .cloned()
            .collect();

        if visible.is_empty() {
            widgets::empty(ui, "No programs match", "Clear the search to see everything.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for app in &visible {
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(widgets::truncate(&app.name, 58))
                                        .size(12.5)
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}",
                                        if app.publisher.is_empty() {
                                            "Unknown publisher"
                                        } else {
                                            &app.publisher
                                        },
                                        if app.version.is_empty() { "—" } else { &app.version }
                                    ))
                                    .size(10.5)
                                    .color(theme::MUTE),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::small_button(ui, "Uninstall").clicked() {
                                        launch = Some(app.clone());
                                    }
                                    ui.add_space(10.0);
                                    ui.label(
                                        RichText::new(if app.size > 0 {
                                            fmt_bytes(app.size)
                                        } else {
                                            "—".to_string()
                                        })
                                        .size(11.0)
                                        .color(theme::MUTE),
                                    );
                                    ui.add_space(10.0);
                                    ui.label(
                                        RichText::new(if app.install_date.is_empty() {
                                            "—".to_string()
                                        } else {
                                            app.install_date.clone()
                                        })
                                        .size(11.0)
                                        .color(theme::MUTE),
                                    );
                                },
                            );
                        });
                        ui.add_space(5.0);
                        widgets::divider(ui);
                    }
                });
        }
    });

    widgets::muted(
        ui,
        "G-OpenCleaner launches each program's own uninstaller. Nothing is force-deleted behind the vendor's back.",
    );

    if let Some(app_entry) = launch {
        if app_entry.uninstall.trim().is_empty() {
            app.toast(
                "No uninstall command",
                "This program registered no uninstaller with Windows.",
                ToastKind::Warn,
            );
        } else {
            app.ask(ConfirmState {
                title: format!("Uninstall {}?", app_entry.name),
                intro: "The vendor's uninstaller will open in its own window. Follow its prompts to finish."
                    .into(),
                rows: Vec::new(),
                actions: Vec::new(),
                warning: None,
                note: None,
                confirm_text: "Launch uninstaller".into(),
                danger: false,
                pending: Pending::Uninstall(
                    app_entry.id.clone(),
                    app_entry.uninstall.clone(),
                    app_entry.name.clone(),
                ),
            });
        }
    }
}
