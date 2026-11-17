//! Background workers. Every expensive call runs on its own thread and reports
//! back through one channel, so the window never blocks.

use crate::core::model::*;
use crate::core::scanner::{self, CleanOpts};
use crate::core::{tools, win};
use std::sync::mpsc::Sender;
use std::thread;

pub struct CleanSummary {
    pub label: String,
    pub bytes: u64,
    pub removed: u64,
    pub skipped: u64,
    /// Part of `bytes` that was moved to quarantine instead of destroyed.
    pub quarantined: u64,
    pub outcomes: Vec<CleanOutcome>,
    pub errors: Vec<String>,
}

pub enum Msg {
    System(Box<SystemInfo>),
    Rules(Vec<Rule>),
    Health(Box<HealthReport>),
    Scanned(String, Vec<ScanResult>),
    Preview(Box<RulePreview>),
    Cleaned(Box<CleanSummary>),
    Startups(Vec<StartupItem>),
    Processes(Vec<ProcRow>),
    Tasks(Vec<TaskRow>),
    Software(Vec<AppEntry>),
    Drivers(Vec<DriverRow>),
    DriverUpdates(Box<DriverUpdates>),
    Upgrades(bool, Vec<UpgradeRow>),
    Cloud(Vec<CloudDrive>),
    Clutter(Box<ClutterReport>),
    Disk(Box<DiskReport>),
    Duplicates(Box<DupReport>),
    Quarantine(Vec<crate::core::quarantine::Run>),
    Elevated(Box<crate::core::elevate::ElevatedReport>),
    Done(String, String),
    Failed(String, String),
}

pub fn spawn_system(tx: Sender<Msg>) {
    thread::spawn(move || {
        let info = win::system_info();
        let _ = tx.send(Msg::System(Box::new(info)));
    });
}

pub fn spawn_rules(tx: Sender<Msg>) {
    thread::spawn(move || {
        let mut rules = crate::core::rules::all();
        for rule in rules.iter_mut() {
            rule.applicable = scanner::is_applicable(rule);
        }
        let _ = tx.send(Msg::Rules(rules));
    });
}

pub fn spawn_health(rules: Vec<Rule>, opts: CleanOpts, tx: Sender<Msg>) {
    thread::spawn(move || {
        let report = health_scan(&rules, &opts);
        let _ = tx.send(Msg::Health(Box::new(report)));
    });
}

pub fn spawn_scan(rules: Vec<Rule>, ids: Vec<String>, opts: CleanOpts, tag: String, tx: Sender<Msg>) {
    thread::spawn(move || {
        let mut results = Vec::new();
        for rule in rules.iter().filter(|r| ids.contains(&r.id)) {
            results.push(scanner::scan_rule(rule, &opts));
        }
        let _ = tx.send(Msg::Scanned(tag, results));
    });
}

/// How many paths the Custom Clean disclosure shows per category.
pub const PREVIEW_ROWS: usize = 40;

pub fn spawn_preview(rule: Rule, opts: CleanOpts, tx: Sender<Msg>) {
    thread::spawn(move || {
        let preview = scanner::preview_rule(&rule, &opts, PREVIEW_ROWS);
        let _ = tx.send(Msg::Preview(Box::new(preview)));
    });
}

pub fn spawn_clean(rules: Vec<Rule>, ids: Vec<String>, opts: CleanOpts, label: String, tx: Sender<Msg>) {
    thread::spawn(move || {
        let mut summary = CleanSummary {
            label,
            bytes: 0,
            removed: 0,
            skipped: 0,
            quarantined: 0,
            outcomes: Vec::new(),
            errors: Vec::new(),
        };
        for rule in rules.iter().filter(|r| ids.contains(&r.id)) {
            let outcome = scanner::clean_rule(rule, &opts);
            summary.bytes += outcome.bytes;
            summary.removed += outcome.removed;
            summary.skipped += outcome.skipped;
            summary.quarantined += outcome.quarantined;
            summary.errors.extend(outcome.errors.iter().cloned());
            summary.outcomes.push(outcome);
        }
        let _ = tx.send(Msg::Cleaned(Box::new(summary)));
    });
}

pub fn spawn_startups(tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Startups(win::startup_items()));
    });
}

pub fn spawn_processes(tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Processes(win::processes()));
    });
}

pub fn spawn_tasks(tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Tasks(win::scheduled_tasks()));
    });
}

pub fn spawn_software(tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Software(win::installed_software()));
    });
}

pub fn spawn_driver_updates(tx: Sender<Msg>) {
    thread::spawn(move || {
        let report = match win::driver_updates() {
            Ok(updates) => DriverUpdates {
                updates,
                error: None,
            },
            Err(e) => DriverUpdates {
                updates: Vec::new(),
                error: Some(e),
            },
        };
        let _ = tx.send(Msg::DriverUpdates(Box::new(report)));
    });
}

pub fn spawn_drivers(tx: Sender<Msg>) {
    thread::spawn(move || match win::drivers() {
        Ok(rows) => {
            let _ = tx.send(Msg::Drivers(rows));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed("drivers".into(), e));
        }
    });
}

pub fn spawn_upgrades(tx: Sender<Msg>) {
    thread::spawn(move || {
        let report = win::winget_upgrades();
        let _ = tx.send(Msg::Upgrades(report.supported, report.items));
    });
}

pub fn spawn_upgrade(id: String, name: String, tx: Sender<Msg>) {
    thread::spawn(move || match win::winget_upgrade(&id) {
        Ok(output) => {
            let _ = tx.send(Msg::Done(name, output));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed(name, e));
        }
    });
}

pub fn spawn_cloud(tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Cloud(tools::cloud_drives()));
    });
}

pub fn spawn_clutter(path: String, tx: Sender<Msg>) {
    thread::spawn(move || {
        let report = tools::cloud_clutter(std::path::Path::new(&path));
        let _ = tx.send(Msg::Clutter(Box::new(report)));
    });
}

pub fn spawn_disk(path: String, depth: usize, tx: Sender<Msg>) {
    thread::spawn(move || {
        let report = tools::analyze_disk(std::path::Path::new(&path), depth);
        let _ = tx.send(Msg::Disk(Box::new(report)));
    });
}

pub fn spawn_duplicates(roots: Vec<String>, min_size: u64, tx: Sender<Msg>) {
    thread::spawn(move || {
        let report = tools::find_duplicates(&roots, min_size);
        let _ = tx.send(Msg::Duplicates(Box::new(report)));
    });
}

pub fn spawn_delete(paths: Vec<String>, roots: Vec<String>, secure: bool, exclusions: Vec<String>, label: String, tx: Sender<Msg>) {
    thread::spawn(move || {
        let opts = CleanOpts {
            secure,
            exclusions,
            extra_roots: roots.into_iter().map(std::path::PathBuf::from).collect(),
        };
        let mut summary = CleanSummary {
            label,
            bytes: 0,
            removed: 0,
            skipped: 0,
            quarantined: 0,
            outcomes: Vec::new(),
            errors: Vec::new(),
        };
        for path in paths {
            // Files a user picked in an analyzer are deleted outright.
            let stats = scanner::remove_path(std::path::Path::new(&path), &opts, None);
            summary.bytes += stats.bytes;
            summary.removed += stats.removed;
            summary.skipped += stats.skipped;
        }
        let _ = tx.send(Msg::Cleaned(Box::new(summary)));
    });
}

pub fn spawn_startup_toggle(item: StartupItem, enabled: bool, tx: Sender<Msg>) {
    thread::spawn(move || match win::set_startup_enabled(&item, enabled) {
        Ok(_) => {
            let label = if enabled { format!("{} enabled", item.name) } else { format!("{} disabled", item.name) };
            let _ = tx.send(Msg::Done(label, String::new()));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed(format!("Could not change {}", item.name), e));
        }
    });
}

pub fn spawn_startup_delete(item: StartupItem, tx: Sender<Msg>) {
    thread::spawn(move || match win::delete_startup_item(&item) {
        Ok(note) => {
            let _ = tx.send(Msg::Done("Startup entry removed".into(), note));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed("Could not remove entry".into(), e));
        }
    });
}

pub fn spawn_kill(pid: u32, name: String, tx: Sender<Msg>) {
    thread::spawn(move || match win::kill_process(pid) {
        Ok(_) => {
            let _ = tx.send(Msg::Done(format!("{} ended", name), String::new()));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed(format!("Could not end {}", name), e));
        }
    });
}

pub fn spawn_uninstall(id: String, command: String, name: String, tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = id;
        match win::launch_uninstaller(&command) {
            Ok(_) => {
                let _ = tx.send(Msg::Done(
                    "Uninstaller started".into(),
                    format!("Follow the prompts in the {} window.", name),
                ));
            }
            Err(e) => {
                let _ = tx.send(Msg::Failed("Could not start the uninstaller".into(), e));
            }
        }
    });
}

/* -------------------------------------------------------------- quarantine */

pub fn spawn_quarantine_list(tx: Sender<Msg>) {
    thread::spawn(move || {
        let _ = tx.send(Msg::Quarantine(crate::core::quarantine::list()));
    });
}

pub fn spawn_restore(run_id: String, stored: String, tx: Sender<Msg>) {
    thread::spawn(move || match crate::core::quarantine::restore(&run_id, &stored) {
        Ok(path) => {
            let _ = tx.send(Msg::Done(
                "File restored".into(),
                path.to_string_lossy().to_string(),
            ));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed("Could not restore".into(), e));
        }
    });
}

pub fn spawn_purge_all(tx: Sender<Msg>) {
    thread::spawn(move || {
        let freed = crate::core::quarantine::purge_all();
        let _ = tx.send(Msg::Done(
            format!("Purged {}", fmt_bytes(freed)),
            "The quarantined files are gone for good.".into(),
        ));
    });
}

/// Runs `task` in a second, elevated copy of this executable. The UAC prompt is
/// raised by that process; this one only waits for its report.
pub fn spawn_elevated(task: String, restore_point: bool, tx: Sender<Msg>) {
    thread::spawn(move || {
        let file = crate::core::elevate::report_path(&task);
        let _ = std::fs::remove_file(&file);
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut args = format!("--elevated {} --report \"{}\"", task, file.display());
        if restore_point {
            args.push_str(" --restore-point");
        }

        let outcome = win::elevate(&exe, &args);
        let report = match outcome {
            Ok(_) => crate::core::elevate::read_report(&file),
            Err(e) => Some(crate::core::elevate::ElevatedReport {
                task: task.clone(),
                ok: false,
                message: e,
                ..Default::default()
            }),
        };
        let _ = std::fs::remove_file(&file);

        let report = report.unwrap_or_else(|| crate::core::elevate::ElevatedReport {
            task: task.clone(),
            ok: false,
            message: "The administrator copy of G-OpenCleaner did not report back.".into(),
            ..Default::default()
        });
        let _ = tx.send(Msg::Elevated(Box::new(report)));
    });
}

pub fn spawn_restore_point(label: String, tx: Sender<Msg>) {
    thread::spawn(move || match win::restore_point(&label) {
        Ok(_) => {
            let _ = tx.send(Msg::Done("Restore point created".into(), String::new()));
        }
        Err(e) => {
            let _ = tx.send(Msg::Failed("Could not create restore point".into(), e));
        }
    });
}

/* ------------------------------------------------------------ health scan */

fn score_for_junk(bytes: u64) -> u32 {
    let gb = bytes as f64 / 1024f64.powi(3);
    if gb > 5.0 {
        40
    } else if gb > 1.0 {
        26
    } else if gb > 0.3 {
        14
    } else if gb > 0.05 {
        6
    } else {
        0
    }
}

fn collect_actions(results: &[ScanResult]) -> Vec<Action> {
    let mut out = Vec::new();
    for result in results {
        for action in &result.actions {
            if !out.contains(action) {
                out.push(*action);
            }
        }
    }
    out
}

pub fn health_scan(rules: &[Rule], opts: &CleanOpts) -> HealthReport {
    let junk_rules: Vec<&Rule> = rules
        .iter()
        .filter(|r| r.default_on && r.group != Group::Browsers && r.health_fix && r.applicable)
        .collect();
    let privacy_rules: Vec<&Rule> = rules
        .iter()
        .filter(|r| {
            r.browser.is_some()
                && r.default_on
                && r.applicable
                && matches!(
                    r.id.rsplit('.').next().unwrap_or(""),
                    "cookies" | "history" | "session" | "forms"
                )
        })
        .collect();

    let junk: Vec<ScanResult> = junk_rules.iter().map(|r| scanner::scan_rule(r, opts)).collect();
    let privacy: Vec<ScanResult> = privacy_rules.iter().map(|r| scanner::scan_rule(r, opts)).collect();

    let junk_bytes: u64 = junk.iter().map(|r| r.size).sum();
    let junk_files: u64 = junk.iter().map(|r| r.count).sum();
    let privacy_items: u64 = privacy.iter().map(|r| r.count).sum();

    let system = win::system_info();
    let startup = win::startup_items();
    let drivers = win::drivers().unwrap_or_default();
    let enabled_startup = startup.iter().filter(|i| i.enabled).count() as u32;

    let drive = system.system_drive();
    let free_bytes = drive.map(|d| d.free).unwrap_or(0);
    let disk_total = drive.map(|d| d.total).unwrap_or(0);
    let free_pct = if disk_total > 0 { free_bytes as f32 / disk_total as f32 * 100.0 } else { 0.0 };
    let mem_used_pct = if system.mem_total > 0 {
        (system.mem_total.saturating_sub(system.mem_free)) as f32 / system.mem_total as f32 * 100.0
    } else {
        0.0
    };

    let cutoff = crate::core::store::now_secs().saturating_sub(3 * 365 * 24 * 3600);
    let old_drivers = drivers.iter().filter(|d| d.old && !d.date.is_empty()).count() as u32;
    let _ = cutoff;

    let mut checks = Vec::new();

    checks.push(HealthCheck {
        id: "junk",
        title: "Junk files",
        detail: if junk_bytes > 0 {
            format!("{} files can be removed", fmt_count(junk_files))
        } else {
            "Nothing to clean up".into()
        },
        value_text: fmt_bytes(junk_bytes),
        status: if junk_bytes > 300 * 1024 * 1024 {
            Status::Bad
        } else if junk_bytes > 40 * 1024 * 1024 {
            Status::Warn
        } else {
            Status::Good
        },
        penalty: score_for_junk(junk_bytes),
        fix_rule_ids: junk
            .iter()
            .filter(|r| r.count > 0)
            .map(|r| r.id.clone())
            .collect(),
        fix_actions: collect_actions(&junk),
        link: None,
    });

    checks.push(HealthCheck {
        id: "privacy",
        title: "Privacy traces",
        detail: if privacy_items > 0 {
            format!("{} browser traces found", fmt_count(privacy_items))
        } else {
            "No browsing traces found".into()
        },
        value_text: format!("{} items", fmt_count(privacy_items)),
        status: if privacy_items > 200 {
            Status::Bad
        } else if privacy_items > 0 {
            Status::Warn
        } else {
            Status::Good
        },
        penalty: if privacy_items > 200 { 15 } else if privacy_items > 0 { 7 } else { 0 },
        fix_rule_ids: privacy
            .iter()
            .filter(|r| r.count > 0)
            .map(|r| r.id.clone())
            .collect(),
        fix_actions: collect_actions(&privacy),
        link: None,
    });

    checks.push(HealthCheck {
        id: "startup",
        title: "Startup impact",
        detail: if enabled_startup > 0 {
            format!("{} programs run at sign-in", enabled_startup)
        } else {
            "Clean start-up".into()
        },
        value_text: format!("{} apps", enabled_startup),
        status: if enabled_startup > 10 {
            Status::Bad
        } else if enabled_startup > 5 {
            Status::Warn
        } else {
            Status::Good
        },
        penalty: if enabled_startup > 10 { 15 } else if enabled_startup > 5 { 7 } else { 0 },
        fix_rule_ids: Vec::new(),
        fix_actions: Vec::new(),
        link: Some("startup"),
    });

    checks.push(HealthCheck {
        id: "disk",
        title: "Disk space",
        detail: format!("{} free of {}", fmt_bytes(free_bytes), fmt_bytes(disk_total)),
        value_text: format!("{:.0}% free", free_pct),
        status: if free_pct < 10.0 {
            Status::Bad
        } else if free_pct < 20.0 {
            Status::Warn
        } else {
            Status::Good
        },
        penalty: if free_pct < 10.0 { 20 } else if free_pct < 20.0 { 10 } else { 0 },
        fix_rule_ids: Vec::new(),
        fix_actions: Vec::new(),
        link: Some("disk"),
    });

    checks.push(HealthCheck {
        id: "memory",
        title: "Memory pressure",
        detail: format!(
            "{} of {} in use",
            fmt_bytes(system.mem_total.saturating_sub(system.mem_free)),
            fmt_bytes(system.mem_total)
        ),
        value_text: format!("{:.0}% used", mem_used_pct),
        status: if mem_used_pct > 88.0 {
            Status::Bad
        } else if mem_used_pct > 72.0 {
            Status::Warn
        } else {
            Status::Good
        },
        penalty: if mem_used_pct > 88.0 { 12 } else if mem_used_pct > 72.0 { 6 } else { 0 },
        fix_rule_ids: Vec::new(),
        fix_actions: Vec::new(),
        link: Some("performance"),
    });

    checks.push(HealthCheck {
        id: "drivers",
        title: "Driver age",
        detail: if old_drivers > 0 {
            format!("{} drivers are over three years old", old_drivers)
        } else {
            "Drivers look recent".into()
        },
        value_text: format!("{} old", old_drivers),
        status: if old_drivers > 8 { Status::Warn } else { Status::Good },
        penalty: if old_drivers > 8 { 6 } else { 0 },
        fix_rule_ids: Vec::new(),
        fix_actions: Vec::new(),
        link: Some("drivers"),
    });

    let penalty: u32 = checks.iter().map(|c| c.penalty).sum();
    let score = 100u32.saturating_sub(penalty).min(100);

    let mut plan_rows = Vec::new();
    let mut plan_actions = Vec::new();
    for result in junk.iter().chain(privacy.iter()) {
        if result.count == 0 && result.actions.is_empty() {
            continue;
        }
        plan_rows.push(PlanRow {
            id: result.id.clone(),
            name: result.name.clone(),
            bytes: result.size,
            count: result.count,
        });
        for action in &result.actions {
            if !plan_actions.contains(action) {
                plan_actions.push(*action);
            }
        }
    }
    plan_rows.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    HealthReport {
        score,
        checks,
        plan_bytes: plan_rows.iter().map(|r| r.bytes).sum(),
        plan_count: plan_rows.iter().map(|r| r.count).sum(),
        plan_rows,
        plan_actions,
        excluded: rules
            .iter()
            .filter(|r| !r.health_fix)
            .map(|r| r.name.clone())
            .collect(),
        junk_bytes,
        junk_files,
        privacy_items,
        startup_items: enabled_startup,
        free_bytes,
        disk_total,
        mem_used_pct,
        old_drivers,
    }
}
