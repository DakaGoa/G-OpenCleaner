//! Application shell: navigation, background message pump, confirmations.

use crate::core::jobs::{self, CleanSummary, Msg};
use crate::core::model::*;
use crate::core::scanner::CleanOpts;
use crate::core::store::Store;
use crate::{theme, views, widgets};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Margin, Pos2, RichText, Rounding, Sense, Stroke, Ui,
    Vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Health,
    Clean,
    Browsers,
    Cloud,
    Disk,
    Duplicates,
    Quarantine,
    Elevated,
    Startup,
    Performance,
    Software,
    Drivers,
    Uninstaller,
    Stats,
    Settings,
}

impl View {
    pub fn title(self) -> &'static str {
        match self {
            View::Health => "Health Check",
            View::Clean => "Custom Clean",
            View::Browsers => "Browser Cleaner",
            View::Cloud => "Cloud Drive Cleaner",
            View::Disk => "Disk Analyzer",
            View::Duplicates => "Duplicate Finder",
            View::Quarantine => "Quarantine",
            View::Elevated => "Administrator Tasks",
            View::Startup => "Startup Manager",
            View::Performance => "Performance Optimizer",
            View::Software => "Software Updater",
            View::Drivers => "Driver Updater",
            View::Uninstaller => "Uninstaller",
            View::Stats => "Statistics",
            View::Settings => "Settings",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            View::Health => "Scan this PC and fix what holds it back.",
            View::Clean => "Choose exactly which traces and leftovers to remove.",
            View::Browsers => "Clear caches and privacy traces for every installed browser.",
            View::Cloud => "Find clutter in your synced cloud folders.",
            View::Disk => "See what is actually using your space.",
            View::Duplicates => "Locate identical files wasting storage.",
            View::Quarantine => "Anything removed in the last seven days, ready to put back.",
            View::Elevated => "The cleanups Windows reserves for an administrator.",
            View::Startup => "Decide what is allowed to launch when you sign in.",
            View::Performance => "Inspect running processes and reclaim resources.",
            View::Software => "Keep installed applications current.",
            View::Drivers => "Review device driver versions and ages.",
            View::Uninstaller => "Remove programs you no longer need.",
            View::Stats => "Everything G-OpenCleaner has cleaned on this PC.",
            View::Settings => "Preferences, exclusions and safety controls.",
        }
    }

    pub fn groups() -> Vec<(&'static str, Vec<View>)> {
        vec![
            ("Overview", vec![View::Health, View::Stats]),
            (
                "Cleaning",
                vec![
                    View::Clean,
                    View::Browsers,
                    View::Cloud,
                    View::Disk,
                    View::Duplicates,
                    View::Quarantine,
                ],
            ),
            ("Performance", vec![View::Startup, View::Performance]),
            ("Software", vec![View::Software, View::Drivers, View::Uninstaller]),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Good,
    Bad,
    Warn,
}

impl ToastKind {
    fn color(self) -> Color32 {
        match self {
            ToastKind::Info => theme::ACCENT,
            ToastKind::Good => theme::GOOD,
            ToastKind::Bad => theme::BAD,
            ToastKind::Warn => theme::WARN,
        }
    }
}

pub struct Toast {
    pub title: String,
    pub body: String,
    pub kind: ToastKind,
    pub born: Instant,
}

#[derive(Clone)]
pub enum Pending {
    CleanRules(Vec<String>, String),
    RestoreEntry(String, String, String),
    PurgeQuarantine,
    ElevatedTask(String),
    DeleteFiles(Vec<String>, Vec<String>, String),
    ResetStats,
    RemoveStartup(StartupItem),
    Kill(u32, String),
    Uninstall(String, String, String),
    Upgrade(String, String),
    RestorePoint,
}

#[derive(Clone)]
pub struct ConfirmState {
    pub title: String,
    pub intro: String,
    pub rows: Vec<(String, String)>,
    pub actions: Vec<String>,
    pub warning: Option<String>,
    pub note: Option<String>,
    pub confirm_text: String,
    pub danger: bool,
    pub pending: Pending,
}

/* ------------------------------------------------------------ screen state */

/// The read-only file list behind an expanded Custom Clean row.
#[derive(Clone, Debug)]
pub enum PreviewState {
    Loading,
    Ready(RulePreview),
    Failed(String),
}

#[derive(Default)]
pub struct CleanScreen {
    pub selected: HashSet<String>,
    pub results: HashMap<String, ScanResult>,
    pub scanning: bool,
    pub analyzed: bool,
    pub query: String,
    pub collapsed: HashSet<String>,
    /// Categories the user opened to see the files behind the number.
    pub expanded: HashSet<String>,
    pub previews: HashMap<String, PreviewState>,
}

#[derive(Default)]
pub struct BrowserScreen {
    pub selected: HashMap<String, HashSet<String>>,
    pub results: HashMap<String, Vec<ScanResult>>,
    pub busy: Option<String>,
}

#[derive(Default)]
pub struct CloudScreen {
    pub drives: Vec<CloudDrive>,
    pub active: Option<String>,
    pub clutter: Option<ClutterReport>,
    pub selected: HashSet<String>,
    pub loading: bool,
    pub scanning: Option<String>,
}

#[derive(Default)]
pub struct DiskScreen {
    pub path: String,
    pub depth: usize,
    pub report: Option<DiskReport>,
    pub selected: HashSet<String>,
    pub loading: bool,
}

#[derive(Default)]
pub struct DupScreen {
    pub roots: Vec<String>,
    pub input: String,
    pub min_mb: u64,
    pub report: Option<DupReport>,
    pub selected: HashSet<String>,
    pub loading: bool,
}

#[derive(Default)]
pub struct StartupScreen {
    pub items: Vec<StartupItem>,
    pub filter: u8,
    pub query: String,
    pub loading: bool,
}

#[derive(Default)]
pub struct PerfScreen {
    pub procs: Vec<ProcRow>,
    pub tasks: Vec<TaskRow>,
    pub query: String,
    pub loading: bool,
}

#[derive(Default)]
pub struct SoftwareScreen {
    pub apps: Vec<AppEntry>,
    pub updates: Option<Vec<UpgradeRow>>,
    pub supported: Option<bool>,
    pub checking: bool,
    pub query: String,
    pub loading: bool,
}

#[derive(Default)]
pub struct DriverScreen {
    pub rows: Vec<DriverRow>,
    pub only_old: bool,
    pub query: String,
    pub loading: bool,
    /// A Windows Update search is running.
    pub checking: bool,
    /// Last read-only Windows Update result, until the next search.
    pub updates: Option<DriverUpdates>,
}

pub struct ElevatedScreen {
    /// Last successful measurement: the sizes the user confirms against.
    pub measured: Option<crate::core::elevate::ElevatedReport>,
    /// Result of the last real task.
    pub result: Option<crate::core::elevate::ElevatedReport>,
    /// Task currently waiting for the UAC prompt or running.
    pub busy: Option<String>,
    pub restore_point: bool,
}

impl Default for ElevatedScreen {
    fn default() -> Self {
        Self {
            measured: None,
            result: None,
            busy: None,
            restore_point: true,
        }
    }
}

#[derive(Default)]
pub struct QuarantineScreen {
    pub runs: Vec<crate::core::quarantine::Run>,
    pub loading: bool,
}

#[derive(Default)]
pub struct UninstallScreen {
    pub apps: Vec<AppEntry>,
    pub query: String,
    pub loading: bool,
}

pub struct App {
    pub view: View,
    pub dark: bool,
    pub store: Store,
    pub system: Option<SystemInfo>,
    pub rules: Vec<Rule>,
    pub rules_loaded: bool,

    pub health: Option<HealthReport>,
    pub health_busy: bool,

    pub clean: CleanScreen,
    pub browsers: BrowserScreen,
    pub cloud: CloudScreen,
    pub disk: DiskScreen,
    pub dups: DupScreen,
    pub startup: StartupScreen,
    pub perf: PerfScreen,
    pub software: SoftwareScreen,
    pub drivers: DriverScreen,
    pub uninstall: UninstallScreen,
    pub quarantine: QuarantineScreen,
    pub elevated: ElevatedScreen,

    pub cleaning: bool,
    pub delete_label: Option<String>,
    pub exclusion_input: String,

    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    pub toasts: Vec<Toast>,
    pub confirm: Option<ConfirmState>,
    pub started: Instant,
    /// Set by OPENCLEAN_CAPTURE=<dir>: screenshots every screen, then closes.
    #[cfg(feature = "capture")]
    pub capture: Option<crate::capture::Run>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let store = Store::load();
        let dark = store.settings.theme != "light";
        theme::apply(&cc.egui_ctx, dark);

        let (tx, rx) = channel();
        let mut app = App {
            view: View::Health,
            dark,
            store,
            system: None,
            rules: Vec::new(),
            rules_loaded: false,
            health: None,
            health_busy: false,
            clean: CleanScreen::default(),
            browsers: BrowserScreen::default(),
            cloud: CloudScreen::default(),
            disk: DiskScreen { path: default_disk_path(), depth: 2, ..Default::default() },
            dups: DupScreen { min_mb: 1, ..Default::default() },
            startup: StartupScreen::default(),
            perf: PerfScreen::default(),
            software: SoftwareScreen::default(),
            drivers: DriverScreen::default(),
            uninstall: UninstallScreen::default(),
            quarantine: QuarantineScreen::default(),
            elevated: ElevatedScreen::default(),
            cleaning: false,
            delete_label: None,
            exclusion_input: String::new(),
            tx: tx.clone(),
            rx,
            toasts: Vec::new(),
            confirm: None,
            started: Instant::now(),
            #[cfg(feature = "capture")]
            capture: std::env::var("OPENCLEAN_CAPTURE")
                .ok()
                .map(|dir| crate::capture::start(std::path::PathBuf::from(dir))),
        };

        jobs::spawn_rules(tx.clone());
        jobs::spawn_system(tx.clone());

        // Seven-day retention: expired quarantine is cleared on every launch.
        std::thread::spawn(|| {
            crate::core::quarantine::purge_expired(crate::core::quarantine::RETENTION_DAYS);
        });

        app
    }

    pub fn clean_opts(&self) -> CleanOpts {
        CleanOpts {
            secure: self.store.settings.secure_delete,
            exclusions: self.store.settings.exclusions.clone(),
            extra_roots: Vec::new(),
        }
    }

    pub fn toast(&mut self, title: impl Into<String>, body: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            title: title.into(),
            body: body.into(),
            kind,
            born: Instant::now(),
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
        self.load_view_data();
    }

    /// First-visit data load for each screen.
    pub fn load_view_data(&mut self) {
        let has_rules = self.rules_loaded;
        match self.view {
            View::Health => {
                if self.health.is_none() && !self.health_busy && has_rules {
                    self.health_busy = true;
                    jobs::spawn_health(self.rules.clone(), self.clean_opts(), self.tx.clone());
                }
            }
            View::Browsers => self.ensure_browser_selection(),
            View::Cloud => {
                if self.cloud.drives.is_empty() && !self.cloud.loading {
                    self.cloud.loading = true;
                    jobs::spawn_cloud(self.tx.clone());
                }
            }
            View::Disk => {
                if self.disk.report.is_none() && !self.disk.loading {
                    self.disk.loading = true;
                    jobs::spawn_disk(self.disk.path.clone(), self.disk.depth, self.tx.clone());
                }
            }
            View::Quarantine => {
                if self.quarantine.runs.is_empty() && !self.quarantine.loading {
                    self.quarantine.loading = true;
                    jobs::spawn_quarantine_list(self.tx.clone());
                }
            }
            View::Duplicates => {
                if self.dups.roots.is_empty() {
                    if let Ok(home) = std::env::var("USERPROFILE") {
                        self.dups.roots.push(format!(r"{}\Downloads", home));
                    }
                }
            }
            View::Startup => {
                if self.startup.items.is_empty() && !self.startup.loading {
                    self.startup.loading = true;
                    jobs::spawn_startups(self.tx.clone());
                }
            }
            View::Performance => {
                if self.perf.procs.is_empty() && !self.perf.loading {
                    self.perf.loading = true;
                    jobs::spawn_processes(self.tx.clone());
                    jobs::spawn_tasks(self.tx.clone());
                }
            }
            View::Software => {
                if self.software.apps.is_empty() && !self.software.loading {
                    self.software.loading = true;
                    jobs::spawn_software(self.tx.clone());
                }
            }
            View::Drivers => {
                if self.drivers.rows.is_empty() && !self.drivers.loading {
                    self.drivers.loading = true;
                    jobs::spawn_drivers(self.tx.clone());
                }
            }
            View::Uninstaller => {
                if self.uninstall.apps.is_empty() && !self.uninstall.loading {
                    self.uninstall.loading = true;
                    jobs::spawn_software(self.tx.clone());
                }
            }
            _ => {}
        }
    }

    fn ensure_browser_selection(&mut self) {
        for rule in &self.rules {
            if let Some(id) = rule.browser_id {
                let entry = self.browsers.selected.entry(id.to_string()).or_default();
                if entry.is_empty() && rule.applicable && rule.default_on {
                    entry.insert(rule.id.clone());
                }
            }
        }
    }

    /* -------------------------------------------------------- message pump */

    fn pump(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::System(info) => self.system = Some(*info),
                Msg::Rules(rules) => {
                    self.rules = rules;
                    self.rules_loaded = true;
                    self.clean.selected = self
                        .rules
                        .iter()
                        .filter(|r| r.applicable && r.default_on)
                        .map(|r| r.id.clone())
                        .collect();
                    self.browsers.selected.clear();
                    self.ensure_browser_selection();
                    self.load_view_data();
                }
                Msg::Health(report) => {
                    self.health = Some(*report);
                    self.health_busy = false;
                }
                Msg::Scanned(tag, results) => {
                    if tag == "clean" {
                        self.clean.results = results.into_iter().map(|r| (r.id.clone(), r)).collect();
                        self.clean.analyzed = true;
                        self.clean.scanning = false;
                        // Sizes just changed: refetch the rows that are open.
                        let opts = self.clean_opts();
                        let expanded: Vec<String> = self.clean.expanded.iter().cloned().collect();
                        self.clean.previews.clear();
                        for id in expanded {
                            if let Some(rule) = self.rules.iter().find(|r| r.id == id).cloned() {
                                self.clean.previews.insert(id.clone(), PreviewState::Loading);
                                jobs::spawn_preview(rule, opts.clone(), self.tx.clone());
                            }
                        }
                    } else if let Some(browser_id) = tag.strip_prefix("browser:") {
                        self.browsers.results.insert(browser_id.to_string(), results);
                        self.browsers.busy = None;
                    }
                }
                Msg::Preview(preview) => {
                    let id = preview.id.clone();
                    if self.clean.expanded.contains(&id) {
                        self.clean.previews.insert(id, PreviewState::Ready(*preview));
                    }
                }
                Msg::Cleaned(summary) => {
                    self.cleaning = false;
                    // What those categories hold has changed; close them.
                    self.clean.expanded.clear();
                    self.clean.previews.clear();
                    self.finish_clean(*summary);
                }
                Msg::Startups(items) => {
                    self.startup.items = items;
                    self.startup.loading = false;
                }
                Msg::Processes(rows) => {
                    self.perf.procs = rows;
                    self.perf.loading = false;
                }
                Msg::Tasks(tasks) => self.perf.tasks = tasks,
                Msg::Software(apps) => {
                    self.software.apps = apps.clone();
                    self.software.loading = false;
                    self.uninstall.apps = apps;
                    self.uninstall.loading = false;
                }
                Msg::Drivers(rows) => {
                    self.drivers.rows = rows;
                    self.drivers.loading = false;
                }
                Msg::DriverUpdates(report) => {
                    self.drivers.checking = false;
                    let (title, body, kind) = match &report.error {
                        Some(error) => (
                            "Windows Update check failed".to_string(),
                            error.clone(),
                            ToastKind::Bad,
                        ),
                        None if report.updates.is_empty() => (
                            "Windows Update".to_string(),
                            "No newer signed driver is offered for this PC.".to_string(),
                            ToastKind::Info,
                        ),
                        None => (
                            "Windows Update".to_string(),
                            format!(
                                "{} driver update(s) offered. Open Windows Update to install them.",
                                report.updates.len()
                            ),
                            ToastKind::Good,
                        ),
                    };
                    self.drivers.updates = Some(*report);
                    self.toast(title, body, kind);
                }
                Msg::Upgrades(supported, items) => {
                    self.software.supported = Some(supported);
                    self.software.updates = Some(items);
                    self.software.checking = false;
                }
                Msg::Cloud(drives) => {
                    self.cloud.drives = drives;
                    self.cloud.loading = false;
                }
                Msg::Clutter(report) => {
                    self.cloud.clutter = Some(*report);
                    self.cloud.scanning = None;
                    self.cloud.selected.clear();
                }
                Msg::Disk(report) => {
                    self.disk.report = Some(*report);
                    self.disk.loading = false;
                }
                Msg::Quarantine(runs) => {
                    self.quarantine.runs = runs;
                    self.quarantine.loading = false;
                }
                Msg::Elevated(report) => {
                    self.elevated.busy = None;
                    let task = report.task.clone();
                    let ok = report.ok;
                    let message = report.message.clone();
                    if task == crate::core::elevate::TASK_MEASURE {
                        self.elevated.measured = Some(*report);
                        if !ok {
                            self.toast("Could not measure", message, ToastKind::Bad);
                        }
                    } else {
                        // The sizes on screen are stale once anything was removed.
                        self.elevated.measured = None;
                        self.elevated.result = Some(*report);
                        self.toast(
                            if ok {
                                "Administrator task finished"
                            } else {
                                "Administrator task stopped"
                            },
                            message,
                            if ok { ToastKind::Good } else { ToastKind::Bad },
                        );
                        self.refresh_after_change();
                    }
                }
                Msg::Duplicates(report) => {
                    self.dups.selected = report
                        .groups
                        .iter()
                        .flat_map(|g| g.files.iter().skip(1).map(|f| f.path.clone()))
                        .collect();
                    self.dups.report = Some(*report);
                    self.dups.loading = false;
                }
                Msg::Done(title, body) => {
                    self.toast(title, body, ToastKind::Good);
                    self.refresh_after_change();
                }
                Msg::Failed(title, body) => {
                    self.toast(title, body, ToastKind::Bad);
                    self.refresh_after_change();
                }
            }
        }
    }

    fn finish_clean(&mut self, summary: CleanSummary) {
        // Only what really left the disk counts as freed; quarantined bytes come
        // back if they are restored.
        let freed = summary.bytes.saturating_sub(summary.quarantined);
        if freed > 0 || summary.removed > 0 {
            self.store.record_clean(freed, summary.removed, &summary.label);
        }
        let mut body = format!("{} files removed", fmt_count(summary.removed));
        if summary.skipped > 0 {
            body.push_str(&format!(" · {} kept", fmt_count(summary.skipped)));
        }
        if let Some(err) = summary.errors.first() {
            body.push_str(&format!(" · {}", err));
        }
        if summary.quarantined > 0 {
            body.push_str(&format!(
                " · {} quarantined for {} days",
                fmt_bytes(summary.quarantined),
                crate::core::quarantine::RETENTION_DAYS
            ));
        }
        let title = match (freed, summary.quarantined) {
            (0, q) if q > 0 => "Moved to quarantine".to_string(),
            (f, q) if q > 0 => format!("Freed {} · {} quarantined", fmt_bytes(f), fmt_bytes(q)),
            (f, _) => format!("Freed {}", fmt_bytes(f)),
        };
        self.toast(title, body, ToastKind::Good);

        self.clean.results.clear();
        self.clean.analyzed = false;
        if matches!(self.view, View::Health) {
            self.health_busy = true;
            jobs::spawn_health(self.rules.clone(), self.clean_opts(), self.tx.clone());
        }
        if matches!(self.view, View::Cloud) {
            if let Some(active) = self.cloud.active.clone() {
                self.cloud.scanning = Some(active.clone());
                jobs::spawn_clutter(active, self.tx.clone());
            }
        }
    }

    fn refresh_after_change(&mut self) {
        match self.view {
            View::Startup => {
                self.startup.loading = true;
                jobs::spawn_startups(self.tx.clone());
            }
            View::Performance => {
                self.perf.loading = true;
                jobs::spawn_processes(self.tx.clone());
            }
            View::Software => {
                self.software.loading = true;
                jobs::spawn_software(self.tx.clone());
            }
            View::Uninstaller => {
                self.uninstall.loading = true;
                jobs::spawn_software(self.tx.clone());
            }
            View::Quarantine => {
                self.quarantine.loading = true;
                jobs::spawn_quarantine_list(self.tx.clone());
            }
            _ => {}
        }
    }

    /* ------------------------------------------------------------- cleanup */

    fn prune_toasts(&mut self) {
        let ttl = Duration::from_secs(5);
        self.toasts.retain(|t| t.born.elapsed() < ttl);
    }

    /* -------------------------------------------------------------- confirm */

    fn dispatch(&mut self, pending: Pending) {
        match pending {
            Pending::CleanRules(ids, label) => {
                self.cleaning = true;
                jobs::spawn_clean(self.rules.clone(), ids, self.clean_opts(), label, self.tx.clone());
            }
            Pending::DeleteFiles(paths, roots, label) => {
                self.cleaning = true;
                jobs::spawn_delete(
                    paths,
                    roots,
                    self.store.settings.secure_delete,
                    self.store.settings.exclusions.clone(),
                    label,
                    self.tx.clone(),
                );
            }
            Pending::RestoreEntry(run_id, stored, label) => {
                let _ = label;
                jobs::spawn_restore(run_id, stored, self.tx.clone());
            }
            Pending::PurgeQuarantine => {
                jobs::spawn_purge_all(self.tx.clone());
            }
            Pending::ElevatedTask(task) => {
                self.elevated.busy = Some(task.clone());
                jobs::spawn_elevated(task, self.elevated.restore_point, self.tx.clone());
            }
            Pending::ResetStats => {
                self.store.reset_stats();
                self.toast("Statistics reset", "", ToastKind::Good);
            }
            Pending::RemoveStartup(item) => {
                jobs::spawn_startup_delete(item, self.tx.clone());
            }
            Pending::Kill(pid, name) => {
                jobs::spawn_kill(pid, name, self.tx.clone());
            }
            Pending::Uninstall(id, command, name) => {
                jobs::spawn_uninstall(id, command, name, self.tx.clone());
            }
            Pending::Upgrade(id, name) => {
                jobs::spawn_upgrade(id, name, self.tx.clone());
            }
            Pending::RestorePoint => {
                jobs::spawn_restore_point("G-OpenCleaner checkpoint".into(), self.tx.clone());
            }
        }
    }

    /// Show a confirmation, unless the user turned asking off for reversible work.
    /// Permanent deletions always ask.
    pub fn ask(&mut self, dialog: ConfirmState) {
        if dialog.danger || self.store.settings.confirm_deletes {
            self.confirm = Some(dialog);
        } else {
            self.dispatch(dialog.pending);
        }
    }
}

/* ------------------------------------------------------------------- eframe */

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        self.prune_toasts();
        #[cfg(feature = "capture")]
        crate::capture::tick(self, ctx);

        let bg = if self.dark { theme::BG } else { theme::LIGHT_BG };
        let panel = if self.dark { theme::PANEL_2 } else { theme::LIGHT_PANEL_2 };

        egui::TopBottomPanel::top("topbar")
            .frame(
                Frame::none()
                    .fill(bg)
                    .inner_margin(Margin::symmetric(24.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(self.view.title())
                                .size(21.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.label(
                            RichText::new(self.view.subtitle())
                                .size(12.0)
                                .color(theme::MUTE),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(if self.dark { "Light theme" } else { "Dark theme" })
                                        .size(12.0)
                                        .color(theme::DIM),
                                )
                                .fill(panel)
                                .stroke(Stroke::new(1.0, theme::LINE))
                                .rounding(Rounding::same(8.0)),
                            )
                            .clicked()
                        {
                            self.dark = !self.dark;
                            theme::apply(ctx, self.dark);
                            self.store.settings.theme =
                                if self.dark { "dark".into() } else { "light".into() };
                            self.store.save();
                        }
                        if let Some(system) = &self.system {
                            let colour = if system.admin { theme::GOOD } else { theme::MUTE };
                            ui.label(
                                RichText::new(if system.admin {
                                    "Administrator"
                                } else {
                                    "Standard user"
                                })
                                .size(11.0)
                                .color(colour),
                            );
                            ui.label(RichText::new("·").size(11.0).color(theme::MUTE));
                            ui.label(
                                RichText::new(truncate(&system.hostname, 22))
                                    .size(11.0)
                                    .color(theme::MUTE),
                            );
                        }
                    });
                });
            });

        egui::SidePanel::left("nav")
            .exact_width(232.0)
            .frame(
                Frame::none()
                    .fill(panel)
                    .inner_margin(Margin::symmetric(12.0, 16.0)),
            )
            .show(ctx, |ui| {
                self.draw_sidebar(ui);
            });

        egui::CentralPanel::default()
            .frame(Frame::none().fill(bg).inner_margin(Margin::symmetric(24.0, 8.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        match self.view {
                            View::Health => views::health::draw(self, ui),
                            View::Clean => views::clean::draw(self, ui),
                            View::Browsers => views::clean::draw_browsers(self, ui),
                            View::Cloud => views::clean::draw_cloud(self, ui),
                            View::Disk => views::storage::draw_disk(self, ui),
                            View::Duplicates => views::storage::draw_duplicates(self, ui),
                            View::Quarantine => views::quarantine::draw(self, ui),
                            View::Elevated => views::elevated::draw(self, ui),
                            View::Startup => views::manage::draw_startup(self, ui),
                            View::Performance => views::manage::draw_performance(self, ui),
                            View::Uninstaller => views::manage::draw_uninstaller(self, ui),
                            View::Software => views::updates::draw_software(self, ui),
                            View::Drivers => views::updates::draw_drivers(self, ui),
                            View::Stats => views::system::draw_stats(self, ui),
                            View::Settings => views::system::draw_settings(self, ui),
                        }
                        ui.add_space(28.0);
                    });
            });

        self.draw_toasts(ctx);
        self.draw_confirm(ctx);

        ctx.request_repaint_after(Duration::from_millis(300));
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", head)
    }
}

/* --------------------------------------------------------------- chrome UI */

impl App {
    fn draw_sidebar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, Rounding::same(9.0), theme::ACCENT);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "OC",
                FontId::proportional(12.0),
                Color32::WHITE,
            );
            ui.vertical(|ui| {
                ui.label(RichText::new("G-OpenCleaner").size(14.0).strong().color(theme::TEXT));
                ui.label(RichText::new(VERSION).size(10.0).color(theme::MUTE));
            });
        });
        ui.add_space(10.0);

        let mut target: Option<View> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 104.0).max(96.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                for (group, items) in View::groups() {
                    ui.label(
                        RichText::new(group.to_uppercase())
                            .size(9.5)
                            .color(theme::MUTE)
                            .strong(),
                    );
                    ui.add_space(1.0);
                    for item in items {
                        if nav_item(ui, self.view == item, item.title()) {
                            target = Some(item);
                        }
                    }
                    ui.add_space(4.0);
                }
                ui.label(RichText::new("SYSTEM").size(9.5).color(theme::MUTE).strong());
                ui.add_space(1.0);
                if nav_item(ui, self.view == View::Elevated, "Administrator Tasks") {
                    target = Some(View::Elevated);
                }
                if nav_item(ui, self.view == View::Settings, "Settings") {
                    target = Some(View::Settings);
                }
            });

        ui.add_space(6.0);
        let system = self.system.clone();
        Frame::none()
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, theme::LINE))
            .rounding(Rounding::same(9.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (name, free, total, admin) = match &system {
                    Some(s) => (
                        s.hostname.clone(),
                        s.system_drive().map(|d| d.free).unwrap_or(0),
                        s.system_drive().map(|d| d.total).unwrap_or(0),
                        s.admin,
                    ),
                    None => ("This PC".into(), 0, 0, false),
                };
                ui.horizontal(|ui| {
                    let dot = if admin { theme::GOOD } else { theme::MUTE };
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(7.0, 7.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 3.5, dot);
                    ui.label(RichText::new(truncate(&name, 20)).size(12.0).color(theme::TEXT));
                });
                ui.add_space(4.0);
                let used = if total > 0 {
                    (total.saturating_sub(free)) as f32 / total as f32
                } else {
                    0.0
                };
                widgets::bar(
                    ui,
                    used,
                    if used > 0.92 {
                        theme::BAD
                    } else if used > 0.8 {
                        theme::WARN
                    } else {
                        theme::ACCENT
                    },
                    ui.available_width(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(if total > 0 {
                        format!("{} free", fmt_bytes(free))
                    } else {
                        "Reading drive…".into()
                    })
                    .size(10.5)
                    .color(theme::MUTE),
                );
            });

        if let Some(view) = target {
            self.set_view(view);
        }
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }
        let toasts: Vec<(String, String, Color32)> = self
            .toasts
            .iter()
            .map(|t| (t.title.clone(), t.body.clone(), t.kind.color()))
            .collect();

        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, [-20.0, -20.0])
            .interactable(true)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down(Align::Max), |ui| {
                    for (title, body, colour) in toasts {
                        Frame::none()
                            .fill(theme::PANEL_2)
                            .stroke(Stroke::new(1.0, theme::LINE))
                            .rounding(Rounding::same(10.0))
                            .inner_margin(Margin::symmetric(14.0, 10.0))
                            .show(ui, |ui| {
                                ui.set_max_width(340.0);
                                ui.horizontal(|ui| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 4.0, colour);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(title)
                                                .size(12.5)
                                                .strong()
                                                .color(theme::TEXT),
                                        );
                                        if !body.is_empty() {
                                            ui.label(
                                                RichText::new(body).size(11.5).color(theme::MUTE),
                                            );
                                        }
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }
                });
            });
    }

    fn draw_confirm(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.confirm.clone() else { return };
        let mut close = false;
        let mut accept = false;

        egui::Window::new(dialog.title.clone())
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .show(ctx, |ui| {
                ui.set_max_width(540.0);
                ui.label(
                    RichText::new(&dialog.title)
                        .size(16.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.add_space(8.0);
                if !dialog.intro.is_empty() {
                    ui.label(RichText::new(&dialog.intro).size(12.5).color(theme::DIM));
                }

                if !dialog.rows.is_empty() {
                    ui.add_space(8.0);
                    for (name, meta) in &dialog.rows {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).size(12.0).color(theme::DIM));
                            ui.with_layout(
                                egui::Layout::right_to_left(Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(meta).size(11.5).color(theme::MUTE),
                                    );
                                },
                            );
                        });
                    }
                }

                if !dialog.actions.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Also runs: {}", dialog.actions.join(", ")))
                            .size(12.0)
                            .color(theme::DIM),
                    );
                }

                if let Some(warning) = &dialog.warning {
                    ui.add_space(10.0);
                    widgets::note(ui, warning, theme::BAD);
                }
                if let Some(note) = &dialog.note {
                    ui.add_space(10.0);
                    widgets::note(ui, note, theme::ACCENT);
                }

                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let confirm = if dialog.danger {
                        widgets::danger_button(ui, &dialog.confirm_text)
                    } else {
                        widgets::primary_button(ui, &dialog.confirm_text)
                    };
                    if confirm.clicked() {
                        accept = true;
                    }
                    if widgets::ghost_button(ui, "Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if accept {
            self.confirm = None;
            self.dispatch(dialog.pending);
        } else if close {
            self.confirm = None;
        }
    }
}

fn default_disk_path() -> String {
    std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\".into())
}

fn nav_item(ui: &mut Ui, active: bool, label: &str) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());

    if active {
        ui.painter()
            .rect_filled(rect, Rounding::same(8.0), theme::ACCENT.gamma_multiply(0.14));
        let bar = egui::Rect::from_min_size(
            Pos2::new(rect.left() + 1.0, rect.top() + 6.0),
            Vec2::new(2.5, rect.height() - 12.0),
        );
        ui.painter().rect_filled(bar, Rounding::same(2.0), theme::ACCENT);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, Rounding::same(8.0), theme::PANEL_3);
    }

    ui.painter().text(
        Pos2::new(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        if active { theme::TEXT } else { theme::DIM },
    );
    response.clicked()
}
