//! Custom Clean, Browser Cleaner and Cloud Drive Cleaner.

use crate::app::{App, ConfirmState, Pending, PreviewState};
use crate::core::jobs;
use crate::core::model::*;
use crate::{theme, widgets};
use eframe::egui::{self, RichText, Ui};
use std::collections::HashSet;

/// The read-only list behind an expanded category: the biggest files first,
/// with the true totals underneath.
fn draw_preview(ui: &mut Ui, preview: &RulePreview) {
    if preview.paths.is_empty() {
        widgets::muted(
            ui,
            if preview.truncated {
                "Stopped before reading any file in this category."
            } else if preview.files == 0 {
                "Nothing in this category right now."
            } else {
                "This category holds whole folders, so there is no file list."
            },
        );
        return;
    }

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Largest {} of {} files",
                preview.paths.len(),
                fmt_count(preview.files)
            ))
            .size(10.0)
            .strong()
            .color(theme::MUTE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(fmt_bytes(preview.total))
                    .size(10.0)
                    .color(theme::MUTE),
            );
        });
    });

    for (path, size) in &preview.paths {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(widgets::truncate(path, 104))
                    .size(10.5)
                    .monospace()
                    .color(theme::DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(fmt_bytes(*size))
                        .size(10.5)
                        .color(theme::MUTE),
                );
            });
        });
    }

    if preview.files > preview.paths.len() as u64 {
        widgets::muted(
            ui,
            format!(
                "…and {} more files, {} in total. Nothing here is touched until you press Clean selected.",
                fmt_count(preview.files - preview.paths.len() as u64),
                fmt_bytes(preview.total)
            ),
        );
    }
    if preview.truncated {
        widgets::muted(
            ui,
            "Reading stopped early to keep this quick — the total is a lower bound.",
        );
    }
}

fn selected_bytes(app: &App) -> u64 {
    app.clean
        .selected
        .iter()
        .map(|id| app.clean.results.get(id).map(|r| r.size).unwrap_or(0))
        .sum()
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    if !app.rules_loaded {
        widgets::card(ui, |ui| widgets::loading(ui, "Loading cleaning rules…"));
        return;
    }

    let rules = app.rules.clone();
    let mut analyze = false;
    let mut request_clean = false;
    let mut select_defaults = false;
    let mut select_none = false;
    let mut select_all = false;

    widgets::card_padded(ui, 12.0, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 300.0).max(180.0);
            ui.allocate_ui(egui::vec2(width, 30.0), |ui| {
                widgets::search(ui, &mut app.clean.query, "Search categories");
            });
            if widgets::small_button(ui, "Defaults").clicked() {
                select_defaults = true;
            }
            if widgets::small_button(ui, "None").clicked() {
                select_none = true;
            }
            if widgets::small_button(ui, "All").clicked() {
                select_all = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary_button_enabled(ui, "Analyze", !app.clean.scanning).clicked() {
                    analyze = true;
                }
                if app.clean.scanning {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
    });

    ui.add_space(4.0);

    let query = app.clean.query.to_lowercase();
    let mut toggle: Option<(String, bool)> = None;
    egui::ScrollArea::vertical()
        .id_salt("clean-list")
        .max_height(430.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for group in Group::all() {
                let group_rules: Vec<&Rule> = rules
                    .iter()
                    .filter(|r| r.applicable && r.group == group)
                    .filter(|r| {
                        query.is_empty()
                            || r.name.to_lowercase().contains(&query)
                            || group.label().to_lowercase().contains(&query)
                            || r.browser.map(|b| b.to_lowercase().contains(&query)).unwrap_or(false)
                    })
                    .collect();
                if group_rules.is_empty() {
                    continue;
                }

                let collapsed = app.clean.collapsed.contains(group.label()) && query.is_empty();
                let chosen = group_rules
                    .iter()
                    .filter(|r| app.clean.selected.contains(&r.id))
                    .count();
                let group_size: u64 = group_rules
                    .iter()
                    .map(|r| app.clean.results.get(&r.id).map(|x| x.size).unwrap_or(0))
                    .sum();

                egui::Frame::none()
                    .fill(theme::PANEL_2)
                    .stroke(egui::Stroke::new(1.0, theme::LINE))
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::same(2.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut checked = chosen > 0;
                            if ui.checkbox(&mut checked, "").changed() {
                                for rule in &group_rules {
                                    if checked {
                                        app.clean.selected.insert(rule.id.clone());
                                    } else {
                                        app.clean.selected.remove(&rule.id);
                                    }
                                }
                            }
                            ui.label(
                                RichText::new(group.label().to_uppercase())
                                    .size(10.5)
                                    .strong()
                                    .color(theme::MUTE),
                            );
                                                        ui.label(
                                RichText::new(format!(
                                    "{} of {} selected",
                                    chosen,
                                    group_rules.len()
                                ))
                                .size(10.5)
                                .color(theme::MUTE),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if collapsed { "Show" } else { "Hide" };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(label).size(11.0).color(theme::ACCENT),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    if collapsed {
                                        app.clean.collapsed.remove(group.label());
                                    } else {
                                        app.clean.collapsed.insert(group.label().to_string());
                                    }
                                }
                                if group_size > 0 {
                                    ui.label(
                                        RichText::new(fmt_bytes(group_size))
                                            .size(11.0)
                                            .color(theme::MUTE),
                                    );
                                }
                            });
                        });

                        if !collapsed {
                            for rule in &group_rules {
                                let result = app.clean.results.get(&rule.id).cloned();
                                let mut checked = app.clean.selected.contains(&rule.id);
                                let expanded = app.clean.expanded.contains(&rule.id);
                                let preview = app.clean.previews.get(&rule.id).cloned();

                                egui::Frame::none()
                                    .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            if ui.checkbox(&mut checked, "").changed() {
                                                if checked {
                                                    app.clean.selected.insert(rule.id.clone());
                                                } else {
                                                    app.clean.selected.remove(&rule.id);
                                                }
                                            }
                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(&rule.name)
                                                            .size(12.5)
                                                            .color(theme::TEXT),
                                                    );
                                                    if rule.warning {
                                                        widgets::pill(ui, "sign-out", theme::WARN);
                                                    }
                                                    if rule.permanent {
                                                        widgets::pill(ui, "permanent", theme::BAD);
                                                    }
                                                });
                                                ui.label(
                                                    RichText::new(&rule.desc)
                                                        .size(10.5)
                                                        .color(theme::MUTE),
                                                );
                                            });
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let text = match &result {
                                                        Some(r) if r.size > 0 => fmt_bytes(r.size),
                                                        Some(r) if !r.actions.is_empty() => {
                                                            "action".to_string()
                                                        }
                                                        Some(_) => "—".to_string(),
                                                        None => String::new(),
                                                    };
                                                    if !text.is_empty() {
                                                        ui.label(
                                                            RichText::new(text)
                                                                .size(11.5)
                                                                .color(theme::DIM),
                                                        );
                                                    }
                                                    // Anything with real files can be opened; a
                                                    // folders-only or action category has no list.
                                                    let can_open = rule
                                                        .targets
                                                        .iter()
                                                        .any(|t| t.mode != Mode::Action);
                                                    if can_open {
                                                        let label =
                                                            if expanded { "Hide files" } else { "Show files" };
                                                        if ui
                                                            .add(
                                                                egui::Button::new(
                                                                    RichText::new(label)
                                                                        .size(10.5)
                                                                        .color(theme::ACCENT),
                                                                )
                                                                .frame(false),
                                                            )
                                                            .clicked()
                                                        {
                                                            toggle =
                                                                Some((rule.id.clone(), !expanded));
                                                        }
                                                    }
                                                },
                                            );
                                        });

                                        if expanded {
                                            egui::Frame::none()
                                                .fill(theme::PANEL_3)
                                                .rounding(egui::Rounding::same(8.0))
                                                .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                                                .show(ui, |ui| {
                                                    ui.set_width(ui.available_width());
                                                    match &preview {
                                                        Some(PreviewState::Ready(p)) => {
                                                            draw_preview(ui, p)
                                                        }
                                                        Some(PreviewState::Failed(err)) => {
                                                            widgets::muted(ui, err.clone())
                                                        }
                                                        _ => widgets::muted(
                                                            ui,
                                                            "Reading the files in this category…",
                                                        ),
                                                    }
                                                });
                                        }
                                    });
                            }
                        }
                    });
                ui.add_space(5.0);
            }
        });

    if let Some((id, expand)) = toggle {
        if expand {
            app.clean.expanded.insert(id.clone());
            app.clean.previews.insert(id.clone(), PreviewState::Loading);
            if let Some(rule) = rules.iter().find(|r| r.id == id).cloned() {
                jobs::spawn_preview(rule, app.clean_opts(), app.tx.clone());
            }
        } else {
            app.clean.expanded.remove(&id);
            app.clean.previews.remove(&id);
        }
    }

    ui.add_space(6.0);
    let total = selected_bytes(app);
    widgets::card_padded(ui, 12.0, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(fmt_bytes(total))
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!(
                        "{} categories selected{}",
                        app.clean.selected.len(),
                        if app.clean.analyzed {
                            format!(
                                " · {} files found",
                                fmt_count(
                                    app.clean
                                        .results
                                        .values()
                                        .map(|r| r.count)
                                        .sum::<u64>()
                                )
                            )
                        } else {
                            String::new()
                        }
                    ))
                    .size(11.0)
                    .color(theme::MUTE),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !app.clean.selected.is_empty() && app.clean.analyzed && !app.cleaning;
                if widgets::primary_button_enabled(ui, "Clean selected", enabled).clicked() {
                    request_clean = true;
                }
            });
        });
    });

    widgets::muted(
        ui,
        "Analysis only measures. Removal happens after you confirm, and nothing outside disposable cache locations is touched.",
    );

    if select_defaults {
        app.clean.selected = rules
            .iter()
            .filter(|r| r.applicable && r.default_on)
            .map(|r| r.id.clone())
            .collect();
    }
    if select_none {
        app.clean.selected.clear();
    }
    if select_all {
        app.clean.selected = rules.iter().filter(|r| r.applicable).map(|r| r.id.clone()).collect();
    }
    if analyze {
        app.clean.scanning = true;
        app.clean.previews.clear();
        jobs::spawn_scan(
            rules.clone(),
            app.clean.selected.iter().cloned().collect(),
            app.clean_opts(),
            "clean".into(),
            app.tx.clone(),
        );
    }
    if request_clean {
        let chosen: Vec<Rule> = rules
            .iter()
            .filter(|r| app.clean.selected.contains(&r.id))
            .cloned()
            .collect();
        let ids: Vec<String> = chosen
            .iter()
            .filter(|r| {
                app.clean
                    .results
                    .get(&r.id)
                    .map(|res| res.size > 0 || !res.actions.is_empty())
                    .unwrap_or(false)
            })
            .filter(|r| !r.targets.is_empty())
            .map(|r| r.id.clone())
            .collect();

        if ids.is_empty() {
            app.toast(
                "Nothing to clean",
                "Analyze the selected categories first.",
                crate::app::ToastKind::Warn,
            );
        } else {
            let permanent: Vec<String> = chosen
                .iter()
                .filter(|r| r.permanent)
                .map(|r| r.name.clone())
                .collect();
            let actions: Vec<String> = chosen
                .iter()
                .flat_map(|r| {
                    app.clean
                        .results
                        .get(&r.id)
                        .map(|res| res.actions.clone())
                        .unwrap_or_default()
                })
                .map(|a| a.label().to_string())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            app.ask(ConfirmState {
                title: "Clean the selected categories?".into(),
                intro: format!(
                    "G-OpenCleaner will remove {} from {} categories.",
                    fmt_bytes(total),
                    ids.len()
                ),
                rows: Vec::new(),
                actions,
                warning: if permanent.is_empty() {
                    None
                } else {
                    Some(format!(
                        "{} destroys files permanently. Anything in there cannot be restored afterwards.",
                        permanent.join(", ")
                    ))
                },
                note: if permanent.is_empty() {
                    Some("Only cache and trace locations are touched. Files in use are skipped.".into())
                } else {
                    None
                },
                confirm_text: if total > 0 {
                    format!("Free {}", fmt_bytes(total))
                } else {
                    "Clean".into()
                },
                danger: !permanent.is_empty(),
                pending: Pending::CleanRules(ids, "Custom Clean".into()),
            });
        }
    }
}

/* --------------------------------------------------------- Browser Cleaner */

pub fn draw_browsers(app: &mut App, ui: &mut Ui) {
    if !app.rules_loaded {
        widgets::card(ui, |ui| widgets::loading(ui, "Detecting installed browsers…"));
        return;
    }

    let rules = app.rules.clone();
    let mut browsers: Vec<(String, String)> = Vec::new();
    for rule in &rules {
        if let (Some(name), Some(id)) = (rule.browser, rule.browser_id) {
            let entry = (id.to_string(), name.to_string());
            if !browsers.contains(&entry) {
                browsers.push(entry);
            }
        }
    }

    widgets::note(
        ui,
        "Browsers must be closed before their databases can be cleared. G-OpenCleaner skips locked files rather than forcing them.",
        theme::ACCENT,
    );
    ui.add_space(6.0);

    let mut pending_action: Option<(String, bool)> = None;

    for pair in browsers.chunks(2) {
        ui.columns(2, |cols| {
            for (index, (id, name)) in pair.iter().enumerate() {
                let col = &mut cols[index];
                let categories: Vec<Rule> = rules
                    .iter()
                    .filter(|r| r.browser_id == Some(id.as_str()) && r.applicable)
                    .cloned()
                    .collect();
                let installed = !categories.is_empty();
                let results = app.browsers.results.get(id).cloned();
                let selected = app.browsers.selected.get(id).cloned().unwrap_or_default();

                egui::Frame::none()
                    .fill(theme::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme::LINE))
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(14.0))
                    .show(col, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).size(14.0).strong().color(theme::TEXT));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if installed {
                                        widgets::pill(ui, "Installed", theme::ACCENT);
                                    } else {
                                        widgets::pill(ui, "Not found", theme::MUTE);
                                    }
                                },
                            );
                        });

                        if !installed {
                            widgets::muted(ui, "No profile data found for this browser.");
                            return;
                        }

                        let size: u64 = results
                            .as_ref()
                            .map(|r| r.iter().map(|x| x.size).sum())
                            .unwrap_or(0);
                        let analyzed = results.is_some();
                        widgets::muted(
                            ui,
                            if analyzed {
                                format!("{} can be cleared", fmt_bytes(size))
                            } else {
                                "Analyze to measure what is stored".to_string()
                            },
                        );
                        ui.add_space(4.0);

                        for category in &categories {
                            let result = results.as_ref().and_then(|r| {
                                r.iter().find(|x| x.id == category.id).cloned()
                            });
                            let mut checked = selected.contains(&category.id);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut checked, "").changed() {
                                    if let Some(set) = app.browsers.selected.get_mut(id) {
                                        if checked {
                                            set.insert(category.id.clone());
                                        } else {
                                            set.remove(&category.id);
                                        }
                                    }
                                }
                                ui.label(
                                    RichText::new(&category.name).size(12.0).color(theme::DIM),
                                );
                                if category.warning {
                                    widgets::pill(ui, "logs you out", theme::WARN);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if let Some(res) = result {
                                            if res.size > 0 {
                                                ui.label(
                                                    RichText::new(fmt_bytes(res.size))
                                                        .size(11.0)
                                                        .color(theme::MUTE),
                                                );
                                            }
                                        }
                                    },
                                );
                            });
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if widgets::small_button(ui, "Analyze").clicked() {
                                pending_action = Some((id.clone(), false));
                            }
                            if widgets::primary_button_enabled(
                                ui,
                                "Clean",
                                analyzed && !selected.is_empty() && !app.cleaning,
                            )
                            .clicked()
                            {
                                pending_action = Some((id.clone(), true));
                            }
                        });
                    });
            }
        });
        ui.add_space(4.0);
    }

    if let Some((id, clean)) = pending_action {
        let ids: Vec<String> = app
            .browsers
            .selected
            .get(&id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        if ids.is_empty() {
            app.toast("Nothing selected", "Pick at least one category.", crate::app::ToastKind::Warn);
        } else if clean {
            let name = rules
                .iter()
                .find(|r| r.browser_id == Some(id.as_str()))
                .and_then(|r| r.browser)
                .unwrap_or("browser")
                .to_string();
            app.ask(ConfirmState {
                title: format!("Clean {}?", name),
                intro: "Selected caches and traces will be removed. Cookies sign you out of sites."
                    .into(),
                rows: Vec::new(),
                actions: Vec::new(),
                warning: None,
                note: Some("Saved passwords are excluded unless you ticked them.".into()),
                confirm_text: "Clean".into(),
                danger: false,
                pending: Pending::CleanRules(ids, format!("{} clean", name)),
            });
        } else {
            app.browsers.busy = Some(id.clone());
            jobs::spawn_scan(
                app.rules.clone(),
                ids,
                app.clean_opts(),
                format!("browser:{}", id),
                app.tx.clone(),
            );
        }
    }
}

/* ----------------------------------------------------- Cloud Drive Cleaner */

pub fn draw_cloud(app: &mut App, ui: &mut Ui) {
    if app.cloud.drives.is_empty() {
        if app.cloud.loading {
            widgets::card(ui, |ui| widgets::loading(ui, "Looking for synced cloud folders…"));
        } else {
            widgets::card(ui, |ui| {
                widgets::empty(
                    ui,
                    "No synced cloud folders found",
                    "Install a desktop client for OneDrive, Dropbox, Google Drive, iCloud or Nextcloud.",
                );
            });
            if widgets::ghost_button(ui, "Scan again").clicked() {
                app.cloud.loading = true;
                jobs::spawn_cloud(app.tx.clone());
            }
        }
        return;
    }

    let drives = app.cloud.drives.clone();
    let mut pick: Option<String> = None;

    for drive in &drives {
        egui::Frame::none()
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0, theme::LINE))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(16.0, 13.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&drive.name).size(14.0).strong().color(theme::TEXT),
                        );
                        widgets::mono(ui, widgets::truncate(&drive.path, 58));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let busy = app.cloud.scanning.as_deref() == Some(drive.path.as_str());
                        if widgets::small_button(
                            ui,
                            if busy { "Scanning…" } else { "Find clutter" },
                        )
                        .clicked()
                        {
                            pick = Some(drive.path.clone());
                        }
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new(format!(
                                "{} · {} files",
                                fmt_bytes(drive.size),
                                fmt_count(drive.files)
                            ))
                            .size(12.5)
                            .color(theme::DIM),
                        );
                    });
                });
            });
        ui.add_space(4.0);
    }

    if let Some(path) = pick {
        app.cloud.active = Some(path.clone());
        app.cloud.scanning = Some(path.clone());
        app.cloud.clutter = None;
        jobs::spawn_clutter(path, app.tx.clone());
    }

    if let Some(clutter) = app.cloud.clutter.clone() {
        ui.add_space(6.0);
        widgets::card(ui, |ui| {
            widgets::section(
                ui,
                &format!("Clutter in {}", widgets::truncate(&clutter.root, 52)),
                &format!("{} scanned. Tick what you want removed.", fmt_bytes(clutter.total)),
            );
            ui.add_space(4.0);

            for category in &clutter.categories {
                ui.label(
                    RichText::new(format!("{} · {}", category.name, fmt_bytes(category.size)))
                        .size(11.5)
                        .strong()
                        .color(theme::MUTE),
                );
                if category.items.is_empty() {
                    widgets::muted(ui, "Nothing here.");
                    ui.add_space(4.0);
                    continue;
                }
                egui::ScrollArea::vertical()
                    .id_salt(category.id)
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for item in category.items.iter().take(40) {
                            let mut checked = app.cloud.selected.contains(&item.path);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        app.cloud.selected.insert(item.path.clone());
                                    } else {
                                        app.cloud.selected.remove(&item.path);
                                    }
                                }
                                ui.label(
                                    RichText::new(widgets::truncate(&item.path, 74))
                                        .size(11.0)
                                        .monospace()
                                        .color(theme::DIM),
                                );
                            });
                        }
                    });
                ui.add_space(6.0);
            }
        });

        ui.add_space(6.0);
        let selected: Vec<String> = app.cloud.selected.iter().cloned().collect();
        let bytes: u64 = clutter
            .categories
            .iter()
            .flat_map(|c| c.items.iter())
            .filter(|i| app.cloud.selected.contains(&i.path))
            .map(|i| i.size)
            .sum();

        widgets::card_padded(ui, 12.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} · {} selected", fmt_bytes(bytes), selected.len()))
                        .size(13.0)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::danger_button(ui, "Delete selected").clicked() {
                        if selected.is_empty() {
                            app.toast("Nothing selected", "", crate::app::ToastKind::Warn);
                        } else if let Some(root) = app.cloud.active.clone() {
                            app.ask(ConfirmState {
                                title: "Delete from the cloud folder?".into(),
                                intro: format!(
                                    "{} files ({}) will be deleted. Your cloud client syncs the deletion to every device.",
                                    selected.len(),
                                    fmt_bytes(bytes)
                                ),
                                rows: Vec::new(),
                                actions: Vec::new(),
                                warning: None,
                                note: None,
                                confirm_text: "Delete".into(),
                                danger: true,
                                pending: Pending::DeleteFiles(
                                    selected.clone(),
                                    vec![root],
                                    "Cloud Drive Cleaner".into(),
                                ),
                            });
                        }
                    }
                });
            });
        });
    }
}
