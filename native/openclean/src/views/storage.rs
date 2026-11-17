//! Disk Analyzer and Duplicate Finder.

use crate::app::{App, ConfirmState, Pending};
use crate::core::jobs;
use crate::core::model::*;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};

/* ----------------------------------------------------------- Disk Analyzer */

pub fn draw_disk(app: &mut App, ui: &mut Ui) {
    let mut analyze = false;
    let mut drill: Option<String> = None;

    widgets::card_padded(ui, 12.0, |ui| {
        ui.horizontal(|ui| {
            let depth_index = app.disk.depth.saturating_sub(1);
            if let Some(choice) = widgets::segmented(
                ui,
                &[("Top level", depth_index == 0), ("Folders", depth_index == 1), ("Deep", depth_index == 2)],
                "disk-depth",
            ) {
                app.disk.depth = choice + 1;
                analyze = true;
            }
            let width = (ui.available_width() - 170.0).max(200.0);
            ui.allocate_ui(egui::vec2(width, 30.0), |ui| {
                widgets::path_input(ui, &mut app.disk.path, r"C:\Users");
            });
            if widgets::small_button(ui, "Analyze").clicked() {
                analyze = true;
            }
            if app.disk.loading {
                ui.add(egui::Spinner::new().size(14.0));
            }
        });
    });

    if let Some(report) = app.disk.report.clone() {
        ui.add_space(5.0);
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(fmt_bytes(report.total))
                            .size(26.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    widgets::muted(
                        ui,
                        format!(
                            "{} files in {}",
                            fmt_count(report.file_count),
                            widgets::truncate(&report.root, 52)
                        ),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if report.truncated {
                        widgets::pill(ui, "partial scan", theme::WARN);
                    } else {
                        widgets::pill(ui, "complete", theme::GOOD);
                    }
                });
            });

            ui.add_space(8.0);
            widgets::divider(ui);
            ui.add_space(6.0);
            widgets::section(ui, "Space by folder", "Click any row to drill into it.");

            let max = report.children.first().map(|c| c.size).unwrap_or(1).max(1);
            for child in &report.children {
                let label = child.path.replace(&report.root, "");
                let label = label.trim_start_matches('\\');
                let label = if label.is_empty() { child.path.as_str() } else { label };
                let response = widgets::bar_row(
                    ui,
                    widgets::truncate(label, 34).as_str(),
                    child.size as f32 / max as f32,
                    &format!("{} · {}", fmt_bytes(child.size), fmt_count(child.files)),
                    true,
                );
                if response.clicked() {
                    drill = Some(child.path.clone());
                }
            }
            if report.children.is_empty() {
                widgets::empty(ui, "Nothing here", "");
            }
        });

        ui.add_space(5.0);
        widgets::card(ui, |ui| {
            widgets::section(ui, "Large files", "Files over 50 MB, biggest first.");
            if report.large_files.is_empty() {
                widgets::empty(ui, "No large files found", "This location has no files over 50 MB.");
            } else {
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for file in &report.large_files {
                            let mut checked = app.disk.selected.contains(&file.path);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        app.disk.selected.insert(file.path.clone());
                                    } else {
                                        app.disk.selected.remove(&file.path);
                                    }
                                }
                                ui.label(
                                    RichText::new(widgets::truncate(&file.path, 74))
                                        .size(11.0)
                                        .monospace()
                                        .color(theme::DIM),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(fmt_bytes(file.size))
                                                .size(11.0)
                                                .color(theme::MUTE),
                                        );
                                    },
                                );
                            });
                        }
                    });

                ui.add_space(6.0);
                let roots = [std::path::PathBuf::from(&report.root)];
                let selected: Vec<String> = app.disk.selected.iter().cloned().collect();
                let allowed: Vec<String> = selected
                    .iter()
                    .filter(|p| {
                        crate::core::scanner::is_safe_to_delete(std::path::Path::new(p), &roots)
                    })
                    .cloned()
                    .collect();
                let blocked = selected.len().saturating_sub(allowed.len());

                if !crate::core::scanner::is_safe_extra_root(std::path::Path::new(&report.root)) {
                    widgets::note(
                        ui,
                        "This is a system location. G-OpenCleaner will not delete here, whatever is selected.",
                        theme::WARN,
                    );
                    ui.add_space(4.0);
                } else if blocked > 0 {
                    widgets::note(
                        ui,
                        &format!(
                            "{} selected item(s) are your own files. Documents, Desktop, Pictures, Videos and Music are protected, so G-OpenCleaner will not delete them.",
                            blocked
                        ),
                        theme::WARN,
                    );
                    ui.add_space(4.0);
                }

                let bytes: u64 = report
                    .large_files
                    .iter()
                    .filter(|f| allowed.contains(&f.path))
                    .map(|f| f.size)
                    .sum();

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} removable · {} selected",
                            fmt_bytes(bytes),
                            selected.len()
                        ))
                        .size(12.0)
                        .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::danger_button(ui, "Delete selected").clicked() {
                            if allowed.is_empty() {
                                app.toast(
                                    "Nothing removable",
                                    "The selected files are protected: personal folders and system locations are never deleted.",
                                    crate::app::ToastKind::Warn,
                                );
                            } else {
                                app.ask(ConfirmState {
                                    title: "Delete these files permanently?".into(),
                                    intro: format!(
                                        "{} files ({}) will be deleted. They do not go to the Recycle Bin.",
                                        allowed.len(),
                                        fmt_bytes(bytes)
                                    ),
                                    rows: Vec::new(),
                                    actions: Vec::new(),
                                    warning: Some(
                                        "These are your own files, not cache. Check the list carefully."
                                            .into(),
                                    ),
                                    note: None,
                                    confirm_text: "Delete permanently".into(),
                                    danger: true,
                                    pending: Pending::DeleteFiles(
                                        allowed.clone(),
                                        vec![report.root.clone()],
                                        "Disk Analyzer".into(),
                                    ),
                                });
                            }
                        }
                    });
                });
            }
        });
    } else if app.disk.loading {
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                &format!("Measuring {}", widgets::truncate(&app.disk.path, 48)),
                "Every file is read for its size only. Large folders can take up to 30 seconds.",
            );
            widgets::loading(ui, "Walking the folder tree…");
        });
    }

    if let Some(path) = drill {
        app.disk.path = path.clone();
        app.disk.loading = true;
        app.disk.selected.clear();
        jobs::spawn_disk(path, app.disk.depth, app.tx.clone());
    } else if analyze {
        app.disk.loading = true;
        jobs::spawn_disk(app.disk.path.clone(), app.disk.depth, app.tx.clone());
    }
}

/* -------------------------------------------------------- Duplicate Finder */

pub fn draw_duplicates(app: &mut App, ui: &mut Ui) {
    let mut scan = false;
    let mut remove_root: Option<String> = None;

    widgets::card(ui, |ui| {
        widgets::section(
            ui,
            "Where to look",
            "Identical files are matched by content, never by name alone.",
        );

        let roots = app.dups.roots.clone();
        if roots.is_empty() {
            widgets::muted(ui, "Add a folder to search.");
        }
        for root in &roots {
            ui.horizontal(|ui| {
                widgets::pill(ui, &widgets::truncate(root, 46), theme::ACCENT);
                if ui
                    .add(
                        egui::Button::new(RichText::new("remove").size(10.5).color(theme::BAD))
                            .frame(false),
                    )
                    .clicked()
                {
                    remove_root = Some(root.clone());
                }
            });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 76.0).max(180.0);
            ui.allocate_ui(egui::vec2(width, 28.0), |ui| {
                widgets::path_input(ui, &mut app.dups.input, r"C:\Users\you\Downloads");
            });
            if widgets::small_button(ui, "Add").clicked() {
                let value = app.dups.input.trim().to_string();
                if !value.is_empty() && !app.dups.roots.contains(&value) {
                    app.dups.roots.push(value);
                    app.dups.input.clear();
                }
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Smallest file").size(11.5).color(theme::MUTE));
            let mb_index = [0.1f64, 1.0, 10.0, 100.0]
                .iter()
                .position(|m| (*m * 1024.0 * 1024.0) as u64 == app.dups.min_mb)
                .unwrap_or(1);
            if let Some(choice) = widgets::segmented(
                ui,
                &[
                    ("100 KB", mb_index == 0),
                    ("1 MB", mb_index == 1),
                    ("10 MB", mb_index == 2),
                    ("100 MB", mb_index == 3),
                ],
                "dup-min",
            ) {
                app.dups.min_mb = ([0.1f64, 1.0, 10.0, 100.0][choice] * 1024.0 * 1024.0) as u64;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary_button_enabled(ui, "Scan for duplicates", !app.dups.loading)
                    .clicked()
                {
                    scan = true;
                }
            });
        });
    });

    if app.dups.report.is_none() && !app.dups.loading {
        ui.add_space(5.0);
        widgets::card(ui, |ui| {
            widgets::empty(
                ui,
                "Nothing scanned yet",
                "Files are compared by content, so copies are found even when they were renamed.",
            );
        });
    }

    if app.dups.loading {
        ui.add_space(5.0);
        widgets::card(ui, |ui| widgets::loading(ui, "Hashing candidate files…"));
    }

    if let Some(report) = app.dups.report.clone() {
        ui.add_space(5.0);
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                &format!("{} duplicate groups", fmt_count(report.groups.len() as u64)),
                &format!(
                    "{} could be reclaimed · {} files scanned",
                    fmt_bytes(report.wasted),
                    fmt_count(report.scanned)
                ),
            );
            if report.groups.is_empty() {
                widgets::empty(ui, "No duplicates found", "Try a lower size threshold or more folders.");
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for group in &report.groups {
                        ui.label(
                            RichText::new(format!(
                                "{} × {} copies · wasted {}",
                                fmt_bytes(group.size),
                                group.files.len(),
                                fmt_bytes(group.wasted)
                            ))
                            .size(11.0)
                            .strong()
                            .color(theme::MUTE),
                        );
                        for file in &group.files {
                            let mut checked = app.dups.selected.contains(&file.path);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        app.dups.selected.insert(file.path.clone());
                                    } else {
                                        app.dups.selected.remove(&file.path);
                                    }
                                }
                                ui.label(
                                    RichText::new(widgets::truncate(&file.path, 72))
                                        .size(11.0)
                                        .monospace()
                                        .color(theme::DIM),
                                );
                            });
                        }
                        ui.add_space(6.0);
                    }
                });

            ui.add_space(4.0);
            let roots: Vec<std::path::PathBuf> =
                app.dups.roots.iter().map(std::path::PathBuf::from).collect();
            let roots_ok = roots
                .iter()
                .all(|r| crate::core::scanner::is_safe_extra_root(r));
            let selected: Vec<String> = app.dups.selected.iter().cloned().collect();
            let allowed: Vec<String> = selected
                .iter()
                .filter(|p| {
                    crate::core::scanner::is_safe_to_delete(std::path::Path::new(p), &roots)
                })
                .cloned()
                .collect();
            let blocked = selected.len().saturating_sub(allowed.len());

            if !roots_ok {
                widgets::note(
                    ui,
                    "One of these folders is a system location. G-OpenCleaner will not delete anything from it.",
                    theme::WARN,
                );
                ui.add_space(4.0);
            } else if blocked > 0 {
                widgets::note(
                    ui,
                    &format!(
                        "{} selected copy/copies are your own files. Documents, Desktop, Pictures, Videos and Music are protected, so G-OpenCleaner will not delete them.",
                        blocked
                    ),
                    theme::WARN,
                );
                ui.add_space(4.0);
            }

            let bytes: u64 = report
                .groups
                .iter()
                .flat_map(|g| g.files.iter())
                .filter(|f| allowed.contains(&f.path))
                .map(|f| f.size)
                .sum();

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} · {} selected", fmt_bytes(bytes), selected.len()))
                        .size(12.0)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::danger_button(ui, "Delete selected").clicked() {
                        if allowed.is_empty() {
                            app.toast(
                                "Nothing removable",
                                "The selected copies are protected: personal folders and system locations are never deleted.",
                                crate::app::ToastKind::Warn,
                            );
                        } else {
                            app.ask(ConfirmState {
                                title: "Delete duplicate copies?".into(),
                                intro: format!(
                                    "{} files ({}) will be permanently deleted. One copy of each file stays behind.",
                                    allowed.len(),
                                    fmt_bytes(bytes)
                                ),
                                rows: Vec::new(),
                                actions: Vec::new(),
                                warning: None,
                                note: None,
                                confirm_text: "Delete copies".into(),
                                danger: true,
                                pending: Pending::DeleteFiles(
                                    allowed.clone(),
                                    app.dups.roots.clone(),
                                    "Duplicate Finder".into(),
                                ),
                            });
                        }
                    }
                });
            });
        });
    }

    if let Some(root) = remove_root {
        app.dups.roots.retain(|r| r != &root);
    }

    if scan {
        if app.dups.roots.is_empty() {
            app.toast("Add a folder first", "", crate::app::ToastKind::Warn);
        } else {
            app.dups.loading = true;
            app.dups.report = None;
            jobs::spawn_duplicates(app.dups.roots.clone(), app.dups.min_mb, app.tx.clone());
        }
    }
}
