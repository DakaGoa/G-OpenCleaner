//! Shared value types used by the scanner, the workers and the UI.

use std::fmt;

pub const VERSION: &str = "1.0.0";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Windows,
    Applications,
    Browsers,
    Network,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Windows => "Windows",
            Group::Applications => "Applications",
            Group::Browsers => "Browser cleaner",
            Group::Network => "Network",
        }
    }
    pub fn all() -> [Group; 4] {
        [Group::Windows, Group::Applications, Group::Browsers, Group::Network]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Clipboard,
    Dns,
    RecycleBin,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Clipboard => "Clear clipboard",
            Action::Dns => "Flush DNS cache",
            Action::RecycleBin => "Empty Recycle Bin",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Delete every child entry of the matched folder.
    Contents,
    /// Delete every file under the matched folder matching `pattern`.
    Recurse,
    /// Delete the matched file(s).
    File,
    /// A named system action.
    Action,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub path: String,
    pub mode: Mode,
    pub pattern: Option<&'static str>,
    pub older_than_hours: Option<u64>,
    pub action: Option<Action>,
}

impl Target {
    pub fn dir(path: impl Into<String>) -> Self {
        Target { path: path.into(), mode: Mode::Contents, pattern: None, older_than_hours: None, action: None }
    }
    pub fn recurse(path: impl Into<String>, pattern: &'static str) -> Self {
        Target { path: path.into(), mode: Mode::Recurse, pattern: Some(pattern), older_than_hours: None, action: None }
    }
    pub fn recurse_old(path: impl Into<String>, hours: u64) -> Self {
        Target { path: path.into(), mode: Mode::Recurse, pattern: None, older_than_hours: Some(hours), action: None }
    }
    pub fn file(path: impl Into<String>) -> Self {
        Target { path: path.into(), mode: Mode::File, pattern: None, older_than_hours: None, action: None }
    }
    pub fn action(action: Action) -> Self {
        Target { path: String::new(), mode: Mode::Action, pattern: None, older_than_hours: None, action: Some(action) }
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub group: Group,
    pub name: String,
    pub desc: String,
    pub default_on: bool,
    /// Safe for Health Check's one-click fix (re-creatable data only).
    pub health_fix: bool,
    /// Destroys user data that cannot be recovered.
    pub permanent: bool,
    /// Signs the user out of sites.
    pub warning: bool,
    pub browser: Option<&'static str>,
    pub browser_id: Option<&'static str>,
    pub targets: Vec<Target>,
    /// Cheap applicability check: at least one target exists on disk.
    pub applicable: bool,
}

#[derive(Clone, Default, Debug)]
pub struct ScanResult {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub count: u64,
    pub samples: Vec<(String, u64)>,
    pub actions: Vec<Action>,
    pub error: Option<String>,
    pub truncated: bool,
}

/// What a rule would remove, for the expandable row in Custom Clean.
#[derive(Clone, Default, Debug)]
pub struct RulePreview {
    pub id: String,
    /// Largest files first, capped to what the UI shows.
    pub paths: Vec<(String, u64)>,
    pub total: u64,
    pub files: u64,
    /// The walk hit its time budget, so the totals are a lower bound.
    pub truncated: bool,
}

#[derive(Clone, Default, Debug)]
pub struct CleanOutcome {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub removed: u64,
    pub skipped: u64,
    /// Part of `bytes` that went to quarantine instead of being destroyed.
    pub quarantined: u64,
    pub actions: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Good,
    Warn,
    Bad,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Good => "Healthy",
            Status::Warn => "Attention",
            Status::Bad => "Action needed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HealthCheck {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub value_text: String,
    pub status: Status,
    pub penalty: u32,
    pub fix_rule_ids: Vec<String>,
    pub fix_actions: Vec<Action>,
    pub link: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct PlanRow {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub count: u64,
}

#[derive(Clone, Debug, Default)]
pub struct HealthReport {
    pub score: u32,
    pub checks: Vec<HealthCheck>,
    pub plan_rows: Vec<PlanRow>,
    pub plan_bytes: u64,
    pub plan_count: u64,
    pub plan_actions: Vec<Action>,
    pub excluded: Vec<String>,
    pub junk_bytes: u64,
    pub junk_files: u64,
    pub privacy_items: u64,
    pub startup_items: u32,
    pub free_bytes: u64,
    pub disk_total: u64,
    pub mem_used_pct: f32,
    pub old_drivers: u32,
}

#[derive(Clone, Default, Debug)]
pub struct Drive {
    pub letter: String,
    pub label: String,
    pub total: u64,
    pub free: u64,
}

#[derive(Clone, Default, Debug)]
pub struct SystemInfo {
    pub hostname: String,
    pub user: String,
    pub os_name: String,
    pub os_version: String,
    pub cpu: String,
    pub cores: u32,
    pub mem_total: u64,
    pub mem_free: u64,
    pub uptime_secs: u64,
    pub admin: bool,
    pub drives: Vec<Drive>,
    pub recycle_count: u64,
    pub recycle_bytes: u64,
    pub gpus: Vec<String>,
}

impl SystemInfo {
    pub fn system_drive(&self) -> Option<&Drive> {
        self.drives
            .iter()
            .find(|d| d.letter.eq_ignore_ascii_case("C:"))
            .or_else(|| self.drives.first())
    }
}

#[derive(Clone, Debug)]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    pub command: String,
    pub hive: String,
    pub key: String,
    pub kind: StartupKind,
    pub enabled: bool,
    pub source: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartupKind {
    Registry,
    Folder,
}

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_date: String,
    pub size: u64,
    pub uninstall: String,
}

#[derive(Clone, Debug)]
pub struct ProcRow {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub memory: u64,
    pub cpu_pct: f32,
    pub threads: u32,
}

#[derive(Clone, Debug)]
pub struct DriverRow {
    pub name: String,
    pub class: String,
    pub version: String,
    pub date: String,
    pub manufacturer: String,
    pub old: bool,
    /// Every HardwareID Windows reports for the device, used to match updates.
    pub hardware_ids: Vec<String>,
}

/// One driver update Windows Update offers for this PC.
#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct WuaUpdate {
    pub title: String,
    pub hardware_id: String,
    pub class: String,
    pub manufacturer: String,
    pub model: String,
    pub date: String,
}

/// Result of one read-only Windows Update agent search.
#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct DriverUpdates {
    pub updates: Vec<WuaUpdate>,
    pub error: Option<String>,
}

/// `PCI\VEN_8086&DEV_A170&SUBSYS_...&REV_31` → `PCI\VEN_8086&DEV_A170`.
pub fn driver_id_base(id: &str) -> String {
    let clean = id.trim().to_uppercase();
    let mut parts = clean.split('\\');
    let bus = parts.next().unwrap_or("");
    let device = parts.next().unwrap_or("");
    if bus.is_empty() || device.is_empty() {
        return clean;
    }
    let device: Vec<&str> = device.split('&').take(2).collect();
    format!("{}\\{}", bus, device.join("&"))
}

/// Whether an installed device — identified by its HardwareID list — is the
/// device this Windows Update driver is for. Full IDs first, then the
/// vendor/device pair, which is how Windows itself groups them.
pub fn driver_matches(installed_ids: &[String], update_id: &str) -> bool {
    let wanted = update_id.trim().to_uppercase();
    if wanted.is_empty() {
        return false;
    }
    let wanted_base = driver_id_base(&wanted);
    installed_ids.iter().any(|have| {
        let have = have.trim().to_uppercase();
        !have.is_empty() && (have == wanted || driver_id_base(&have) == wanted_base)
    })
}

#[derive(Clone, Debug)]
pub struct UpgradeRow {
    pub name: String,
    pub id: String,
    pub version: String,
    pub available: String,
}

#[derive(Clone, Debug)]
pub struct TaskRow {
    pub name: String,
    pub path: String,
    pub state: String,
}

#[derive(Clone, Debug)]
pub struct CloudDrive {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub partial: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ClutterCategory {
    pub id: &'static str,
    pub name: &'static str,
    pub size: u64,
    pub items: Vec<FileItem>,
}

#[derive(Clone, Debug)]
pub struct FileItem {
    pub path: String,
    pub size: u64,
    pub mtime: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ClutterReport {
    pub root: String,
    pub total: u64,
    pub categories: Vec<ClutterCategory>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DiskReport {
    pub root: String,
    pub total: u64,
    pub file_count: u64,
    pub children: Vec<Bucket>,
    pub large_files: Vec<FileItem>,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct Bucket {
    pub path: String,
    pub size: u64,
    pub files: u64,
}

#[derive(Clone, Debug, Default)]
pub struct DupReport {
    pub groups: Vec<DupGroup>,
    pub wasted: u64,
    pub scanned: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct DupGroup {
    pub size: u64,
    pub wasted: u64,
    pub files: Vec<FileItem>,
}

pub fn fmt_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 || unit == 0 {
        format!("{:.0} {}", value, units[unit])
    } else {
        format!("{:.1} {}", value, units[unit])
    }
}

pub fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn time_ago(secs: u64) -> String {
    let now = crate::core::store::now_secs();
    if secs == 0 || secs > now {
        return "never".into();
    }
    let d = now - secs;
    if d < 60 {
        "just now".into()
    } else if d < 3600 {
        format!("{} min ago", d / 60)
    } else if d < 86400 {
        format!("{} h ago", d / 3600)
    } else {
        format!("{} d ago", d / 86400)
    }
}

pub fn fmt_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{}d {}h", d, h)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", m)
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
