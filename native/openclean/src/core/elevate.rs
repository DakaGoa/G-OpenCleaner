//! Tasks that need administrator rights.
//!
//! The window never runs elevated code. It starts a second copy of
//! `openclean.exe` with the shell's `runas` verb — that is what raises the UAC
//! prompt — and the copy does the work and writes a JSON report. The window only
//! reads the report. If the prompt is dismissed, nothing runs at all.

use crate::core::model::fmt_bytes;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const TASK_MEASURE: &str = "measure";
pub const TASK_WINDOWS_OLD: &str = "windows-old";
pub const TASK_COMPONENT_STORE: &str = "component-store";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ElevatedReport {
    pub task: String,
    pub ok: bool,
    pub message: String,
    /// Bytes released by the task.
    pub freed: u64,
    /// Readable detail lines for the screen.
    pub rows: Vec<(String, String)>,
    /// Numbers the screen keeps: `windows_old`, `store_actual`, `store_reclaimable`.
    pub values: Vec<(String, u64)>,
}

impl ElevatedReport {
    pub fn value(&self, key: &str) -> Option<u64> {
        self.values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| *v)
    }
}

/* ------------------------------------------------------------------- paths */

/// The previous installation, where Windows always keeps it:
/// `<systemdrive>\Windows.old`.
pub fn windows_old_path() -> PathBuf {
    let win = PathBuf::from(crate::core::win::windows_dir());
    let drive = win
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(r"C:\"));
    drive.join("Windows.old")
}

/// Only that exact folder may ever be removed. Anything else — a parent, a
/// child, a sibling with a similar name — is refused.
pub fn is_windows_old(target: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .to_lowercase()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string()
    };
    target
        .file_name()
        .map(|n| n.eq_ignore_ascii_case("Windows.old"))
        .unwrap_or(false)
        && norm(target) == norm(&windows_old_path())
}

pub fn report_path(task: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openclean-elevated-{}-{}.json",
        std::process::id(),
        task
    ))
}

pub fn read_report(path: &Path) -> Option<ElevatedReport> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn write_report(path: &Path, report: &ElevatedReport) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(report) {
        let _ = fs::write(path, json);
    }
}

/* ------------------------------------------------------------- measurement */

fn folder_size(path: &Path) -> (u64, u64) {
    let mut bytes = 0;
    let mut files = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                bytes += meta.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// `"1.60 GB"` → bytes. Used on DISM's own numbers.
pub fn parse_size(text: &str) -> Option<u64> {
    let cleaned = text.trim().replace(',', ".");
    let mut parts = cleaned.split_whitespace();
    let number: f64 = parts.next()?.parse().ok()?;
    let unit = parts.next().unwrap_or("bytes").to_lowercase();
    let scale = match unit.as_str() {
        "byte" | "bytes" => 1.0,
        "kb" => 1024.0,
        "mb" => 1024.0 * 1024.0,
        "gb" => 1024.0 * 1024.0 * 1024.0,
        "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * scale) as u64)
}

fn dism_label(output: &str, label: &str) -> Option<u64> {
    let wanted = label.to_lowercase();
    output
        .lines()
        .find(|line| line.trim_start().to_lowercase().starts_with(&wanted))
        .and_then(|line| line.split(':').nth(1))
        .and_then(parse_size)
}

fn dism(args: &[&str]) -> Result<String, String> {
    let out = Command::new("dism.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("DISM could not start: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);
    let line = combined
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.contains("Error: 0x0"))
        .last()
        .unwrap_or("DISM failed.");
    Err(line.chars().take(300).collect())
}

/* ------------------------------------------------------------------ tasks */

/// True only when this process really holds an elevated administrator token.
/// `runas` gets us there; starting `--elevated` by hand does not.
pub fn is_elevated() -> bool {
    use windows_sys::Win32::UI::Shell::IsUserAnAdmin;
    unsafe { IsUserAnAdmin() != 0 }
}

/// A measurement only counts when it produced at least one real number.
pub fn measured_anything(values: &[(String, u64)]) -> bool {
    !values.is_empty()
}

pub fn measure(report: &mut ElevatedReport) {
    let old = windows_old_path();
    if old.is_dir() {
        let (bytes, files) = folder_size(&old);
        report.values.push(("windows_old".into(), bytes));
        report.rows.push((
            "Previous Windows installation".into(),
            format!("{} · {} files", fmt_bytes(bytes), files),
        ));
    } else {
        report.rows.push((
            "Previous Windows installation".into(),
            "Not kept on this PC".into(),
        ));
    }

    let mut problems: Vec<String> = Vec::new();

    match dism(&["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"]) {
        Ok(output) => {
            let actual = dism_label(&output, "Actual Size of Component Store");
            if let Some(bytes) = actual {
                report.values.push(("store_actual".into(), bytes));
                report
                    .rows
                    .push(("Component store".into(), fmt_bytes(bytes)));
            }
            let backups =
                dism_label(&output, "Backups and Disabled Features").unwrap_or(0);
            let cache = dism_label(&output, "Cache and Temporary Data").unwrap_or(0);
            let reclaimable = backups + cache;
            if reclaimable > 0 {
                report
                    .values
                    .push(("store_reclaimable".into(), reclaimable));
                report.rows.push((
                    "Reclaimable now".into(),
                    format!("about {}", fmt_bytes(reclaimable)),
                ));
            } else if actual.is_some() {
                // The store has a size but nothing spare to remove. That is a
                // measurement, not a missing one, and the screen must say so.
                report.values.push(("store_reclaimable".into(), 0));
                report
                    .rows
                    .push(("Reclaimable now".into(), "Nothing to reclaim".into()));
            } else {
                report.rows.push((
                    "Component store".into(),
                    "DISM reported no size.".into(),
                ));
                problems.push("DISM reported no size for the component store.".into());
            }
        }
        Err(e) => {
            report.rows.push(("Component store".into(), e.clone()));
            problems.push(format!("DISM: {}", e));
        }
    }

    // An empty report is a failure, not a measurement of nothing.
    if measured_anything(&report.values) {
        report.ok = true;
        report.message = "Measured what an administrator can reclaim.".into();
    } else {
        report.ok = false;
        report.message = if problems.is_empty() {
            "Windows did not report a size for anything on this PC, so there is nothing to confirm."
                .to_string()
        } else {
            format!("Nothing was measured: {}", problems.join(" "))
        };
    }
}

fn remove_windows_old(report: &mut ElevatedReport) {
    let path = windows_old_path();
    if !is_windows_old(&path) {
        report.message = "Refused: that path is not Windows.old.".into();
        return;
    }
    let Ok(meta) = fs::symlink_metadata(&path) else {
        report.message = "There is no Windows.old on this PC.".into();
        return;
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        report.message = "Windows.old is not a plain folder, so G-OpenCleaner will not touch it.".into();
        return;
    }

    let (before, files) = folder_size(&path);
    report
        .rows
        .push(("Before".into(), format!("{} · {} files", fmt_bytes(before), files)));

    // Windows keeps the old installation owned by TrustedInstaller, so unlike
    // everything else in this app it has to be taken over before it can go.
    let p = path.to_string_lossy().to_string();
    let _ = Command::new("takeown.exe")
        .args(["/F", &p, "/R", "/A", "/D", "Y"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let _ = Command::new("icacls.exe")
        .args([&p, "/grant", "*S-1-5-32-544:F", "/T", "/C", "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let mut removed = fs::remove_dir_all(&path).is_ok();
    if !removed {
        let _ = Command::new("cmd.exe")
            .args(["/C", "rd", "/S", "/Q", &p])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        removed = !path.exists();
    }

    let remaining = if path.exists() {
        folder_size(&path).0
    } else {
        0
    };
    report.freed = before.saturating_sub(remaining);

    if removed && !path.exists() {
        report.ok = true;
        report.message = format!("Windows.old is gone, {} freed.", fmt_bytes(report.freed));
    } else {
        report.message = format!(
            "Windows would not release every file. {} of {} was freed; the rest is still there.",
            fmt_bytes(report.freed),
            fmt_bytes(before)
        );
    }
}

fn clean_component_store(report: &mut ElevatedReport) {
    let before = dism(&["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"])
        .ok()
        .and_then(|o| dism_label(&o, "Actual Size of Component Store"));

    match dism(&[
        "/Online",
        "/Cleanup-Image",
        "/StartComponentCleanup",
        "/ResetBase",
    ]) {
        Ok(output) => {
            let after = dism(&["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"])
                .ok()
                .and_then(|o| dism_label(&o, "Actual Size of Component Store"));
            if let (Some(from), Some(to)) = (before, after) {
                report.freed = from.saturating_sub(to);
                report.rows.push((
                    "Component store".into(),
                    format!("{} → {}", fmt_bytes(from), fmt_bytes(to)),
                ));
            }
            let tail: Vec<&str> = output
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.contains('\u{8}'))
                .collect();
            if let Some(last) = tail.last() {
                report.rows.push((
                    "DISM".into(),
                    last.chars().take(200).collect::<String>(),
                ));
            }
            report.ok = true;
            report.message = if report.freed > 0 {
                format!(
                    "Component store cleanup finished, {} freed.",
                    fmt_bytes(report.freed)
                )
            } else {
                "Component store cleanup finished. Windows reported nothing new to remove."
                    .into()
            };
        }
        Err(e) => report.message = format!("DISM did not finish: {}", e),
    }
}

/* -------------------------------------------------------------- entry point */

fn value_of(args: &[String], key: &str) -> Option<String> {
    let index = args.iter().position(|a| a == key)?;
    args.get(index + 1).cloned()
}

/// Runs in the second, elevated copy of the app:
/// `openclean.exe --elevated <task> [--restore-point] --report <file>`.
pub fn run_cli(args: &[String]) -> i32 {
    let task = value_of(args, "--elevated").unwrap_or_default();
    let file = value_of(args, "--report")
        .map(PathBuf::from)
        .unwrap_or_else(|| report_path(&task));
    let restore = args.iter().any(|a| a == "--restore-point");

    let mut report = ElevatedReport {
        task: task.clone(),
        ..Default::default()
    };

    // The helper only trusts itself with admin rights. Anything else must not
    // claim a result, or the window would show "success" for work that never
    // happened.
    if !is_elevated() {
        report.ok = false;
        report.message = "Windows did not give this copy administrator rights, so nothing was read and nothing was changed. Start the task from the Administrator Tasks screen, where Windows asks for approval."
            .into();
        write_report(&file, &report);
        return 1;
    }

    if task == TASK_MEASURE {
        measure(&mut report);
    } else {
        if restore {
            match crate::core::win::restore_point("G-OpenCleaner: before an administrator cleanup") {
                Ok(_) => report.rows.push(("Restore point".into(), "Created".into())),
                Err(e) => {
                    report.message = format!(
                        "Stopped before changing anything: the restore point could not be created ({}). Run the task again with the restore point unchecked to continue without one.",
                        e
                    );
                    write_report(&file, &report);
                    return 1;
                }
            }
        }
        match task.as_str() {
            TASK_WINDOWS_OLD => remove_windows_old(&mut report),
            TASK_COMPONENT_STORE => clean_component_store(&mut report),
            other => report.message = format!("Unknown administrator task: {}", other),
        }
    }

    write_report(&file, &report);
    if report.ok {
        0
    } else {
        1
    }
}
