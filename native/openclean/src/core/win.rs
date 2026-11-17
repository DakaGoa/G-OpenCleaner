//! Windows integration.
//!
//! Native where it matters — filesystem, registry (winreg), process control
//! (Toolhelp + psapi), clipboard and Recycle Bin (shell32). WMI metadata that
//! has no sane Win32 equivalent (driver versions, scheduled tasks, OS summary)
//! is read through PowerShell, exactly as CCleaner's own service uses WMI.

use crate::core::model::*;
use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How long the read-only Windows Update driver search may take before its
/// helper process is killed and the screen offers a retry.
const DRIVER_SEARCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/* --------------------------------------------------------------- helpers */

fn ps(script: &str) -> Result<String, String> {
    let prelude = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Continue'; ";
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!("{}{}", prelude, script),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("PowerShell could not start: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() && stdout.trim().is_empty() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(err.trim().chars().take(300).collect());
    }
    Ok(stdout)
}

fn ps_json(script: &str) -> Result<Value, String> {
    ps_json_from(&ps(script)?)
}

fn ps_json_from(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Value::Null);
    }
    let start = trimmed
        .find(['{', '['])
        .ok_or_else(|| "Unexpected PowerShell output".to_string())?;
    serde_json::from_str(&trimmed[start..]).map_err(|e| format!("Bad JSON from PowerShell: {}", e))
}

/// `ps_json`, but the child is killed if it has not answered within `timeout`.
/// Windows Update can stall with no reply at all, and without this the caller
/// would wait forever with no way to get back to a usable screen.
pub(crate) fn ps_json_timeout(
    script: &str,
    timeout: std::time::Duration,
) -> Result<Value, String> {
    use std::io::Read;
    use std::time::Instant;

    let prelude = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Continue'; ";
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!("{}{}", prelude, script),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("PowerShell could not start: {}", e))?;

    // Drain both pipes on their own threads, so a talkative child never blocks.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = out_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    });
    let err_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = err_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    });

    let deadline = Instant::now() + timeout;
    let mut expired = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    expired = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("could not wait for Windows Update: {}", e));
            }
        }
    };

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();

    if expired || status.is_none() {
        return Err(format!(
            "Windows Update did not answer within {} seconds. The service may be busy or offline — press Check Windows Update to try again.",
            timeout.as_secs()
        ));
    }
    if let Some(status) = status {
        if !status.success() && stdout.trim().is_empty() {
            let message = stderr.trim();
            return Err(if message.is_empty() {
                "Windows Update could not be reached.".into()
            } else {
                message.chars().take(300).collect()
            });
        }
    }
    ps_json_from(&stdout)
}

fn as_array(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(items) => items.iter().collect(),
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .map(|x| match x {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

fn num_of(v: &Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0.0)
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/* ------------------------------------------------------------ system info */

pub fn system_info() -> SystemInfo {
    let script = r#"
$os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
$cs = Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue
$cpu = (Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1).Name
$rb = $null
try { $rb = (New-Object -ComObject Shell.Application).NameSpace(0xA) } catch { }
$rbItems = @()
$rbSize = 0
if ($rb) { $rbItems = @($rb.Items()); foreach ($i in $rbItems) { $rbSize += [double]$i.Size } }
$drives = @()
foreach ($d in Get-CimInstance Win32_LogicalDisk -Filter "DriveType=3" -ErrorAction SilentlyContinue) {
  $drives += [pscustomobject]@{ letter = $d.DeviceID; label = $d.VolumeName; total = [double]$d.Size; free = [double]$d.FreeSpace }
}
$gpus = @()
foreach ($g in Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue) { if ($g.Name) { $gpus += $g.Name } }
[pscustomobject]@{
  hostname   = $env:COMPUTERNAME
  user       = $env:USERNAME
  osName     = if ($os) { $os.Caption.Trim() } else { "Windows" }
  osVersion  = if ($os) { "$($os.Version) / $($os.BuildNumber)" } else { "" }
  cpu        = if ($cpu) { $cpu.Trim() } else { "Unknown CPU" }
  cores      = [int]$env:NUMBER_OF_PROCESSORS
  memTotal   = [double]$cs.TotalPhysicalMemory
  memFree    = [double]$os.FreePhysicalMemory * 1024
  uptime     = [int]((Get-Date) - $os.LastBootUpTime).TotalSeconds
  admin      = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  drives     = $drives
  gpus       = $gpus
  rbCount    = $rbItems.Count
  rbSize     = $rbSize
} | ConvertTo-Json -Depth 4 -Compress
"#;

    let mut info = SystemInfo::default();
    if let Ok(v) = ps_json(script) {
        info.hostname = str_of(&v, "hostname");
        info.user = str_of(&v, "user");
        info.os_name = str_of(&v, "osName");
        info.os_version = str_of(&v, "osVersion");
        info.cpu = str_of(&v, "cpu");
        info.cores = num_of(&v, "cores") as u32;
        info.mem_total = num_of(&v, "memTotal") as u64;
        info.mem_free = num_of(&v, "memFree") as u64;
        info.uptime_secs = num_of(&v, "uptime") as u64;
        info.admin = v.get("admin").and_then(|b| b.as_bool()).unwrap_or(false);
        info.recycle_count = num_of(&v, "rbCount") as u64;
        info.recycle_bytes = num_of(&v, "rbSize") as u64;
        if let Some(drives) = v.get("drives") {
            for d in as_array(drives) {
                info.drives.push(Drive {
                    letter: str_of(d, "letter"),
                    label: str_of(d, "label"),
                    total: num_of(d, "total") as u64,
                    free: num_of(d, "free") as u64,
                });
            }
        }
        if let Some(gpus) = v.get("gpus") {
            info.gpus = as_array(gpus)
                .iter()
                .map(|g| g.as_str().unwrap_or_default().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    if info.hostname.is_empty() {
        info.hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This PC".into());
    }
    info.drives.sort_by(|a, b| a.letter.cmp(&b.letter));
    info
}

/* --------------------------------------------------------------- startup */

const RUN_KEYS: [(&str, &str); 4] = [
    ("HKCU", r"Software\Microsoft\Windows\CurrentVersion\Run"),
    ("HKLM", r"Software\Microsoft\Windows\CurrentVersion\Run"),
    ("HKLM", r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run"),
    ("HKCU", r"Software\Microsoft\Windows\CurrentVersion\RunOnce"),
];

const BACKUP_KEY: &str = r"Software\OpenClean\DisabledStartup";

fn hive_root(hive: &str) -> RegKey {
    if hive.eq_ignore_ascii_case("HKLM") {
        RegKey::predef(HKEY_LOCAL_MACHINE)
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
    }
}

fn startup_item_id(hive: &str, key: &str, name: &str) -> String {
    format!("{}|{}|{}", hive, key, name)
}

pub fn startup_items() -> Vec<StartupItem> {
    let mut items = Vec::new();

    for (hive, key) in RUN_KEYS {
        let Ok(k) = hive_root(hive).open_subkey_with_flags(key, KEY_READ) else { continue };
        for value in k.enum_values().flatten() {
            let name = value.0.clone();
            let Ok(cmd) = k.get_value::<String, _>(&name) else { continue };
            items.push(StartupItem {
                id: startup_item_id(hive, key, &name),
                name,
                command: cmd,
                hive: hive.to_string(),
                key: key.to_string(),
                kind: StartupKind::Registry,
                enabled: true,
                source: format!(
                    "{} \\ {}",
                    hive,
                    key.rsplit('\\').next().unwrap_or(key)
                ),
            });
        }
    }

    // Startup folder shortcuts.
    let folders = [
        (std::env::var("APPDATA").unwrap_or_default(), "CURRENT USER"),
        (std::env::var("ProgramData").unwrap_or_default(), "ALL USERS"),
    ];
    for (base, source) in folders {
        if base.is_empty() {
            continue;
        }
        let dir = Path::new(&base)
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_file() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let display = strip_shortcut_ext(&file_name);
            let full = entry.path().to_string_lossy().to_string();
            items.push(StartupItem {
                id: format!("folder|{}", full),
                name: display,
                command: full,
                hive: source.to_string(),
                key: "Startup folder".into(),
                kind: StartupKind::Folder,
                enabled: true,
                source: "Startup folder".into(),
            });
        }
    }

    // Items this app disabled, kept restorable in our own key.
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(BACKUP_KEY, KEY_READ) {
        for entry in k.enum_keys().flatten() {
            let Ok(sub) = k.open_subkey_with_flags(&entry, KEY_READ) else { continue };
            let name: String = sub.get_value("OriginalName").unwrap_or_default();
            let command: String = sub.get_value("Command").unwrap_or_default();
            let hive: String = sub.get_value("Hive").unwrap_or_default();
            let source_key: String = sub.get_value("SourceKey").unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            items.push(StartupItem {
                id: startup_item_id(&hive, &source_key, &name),
                name,
                command,
                hive: hive.clone(),
                key: source_key.clone(),
                kind: StartupKind::Registry,
                enabled: false,
                source: format!(
                    "{} \\ {} (disabled)",
                    hive,
                    source_key.rsplit('\\').next().unwrap_or(&source_key)
                ),
            });
        }
    }

    items
}

fn strip_shortcut_ext(name: &str) -> String {
    for ext in [".lnk", ".url", ".exe", ".bat", ".cmd"] {
        if name.to_ascii_lowercase().ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

pub fn set_startup_enabled(item: &StartupItem, enabled: bool) -> Result<(), String> {
    if item.kind == StartupKind::Folder {
        return Err("Startup folder shortcuts are removed, not toggled.".into());
    }
    let backup_name = format!("{:016x}", fnv1a(&item.id));
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    if enabled {
        let backup = hkcu
            .open_subkey_with_flags(format!(r"{}\{}", BACKUP_KEY, backup_name), KEY_READ | KEY_WRITE)
            .map_err(|_| "This item is no longer in the disabled list.".to_string())?;
        let name: String = backup.get_value("OriginalName").map_err(|e| e.to_string())?;
        let command: String = backup.get_value("Command").map_err(|e| e.to_string())?;
        let hive: String = backup.get_value("Hive").map_err(|e| e.to_string())?;
        let source_key: String = backup.get_value("SourceKey").map_err(|e| e.to_string())?;

        let target = hive_root(&hive)
            .open_subkey_with_flags(&source_key, KEY_WRITE)
            .map_err(|_| access_hint(&hive))?;
        target
            .set_value(&name, &command)
            .map_err(|_| access_hint(&hive))?;
        let _ = hkcu.delete_subkey_all(format!(r"{}\{}", BACKUP_KEY, backup_name));
        return Ok(());
    }

    let source = hive_root(&item.hive)
        .open_subkey_with_flags(&item.key, KEY_READ | KEY_WRITE)
        .map_err(|_| access_hint(&item.hive))?;
    let command: String = source
        .get_value(&item.name)
        .map_err(|_| "Startup entry not found.".to_string())?;

    let (backup, _) = hkcu
        .create_subkey(format!(r"{}\{}", BACKUP_KEY, backup_name))
        .map_err(|e| e.to_string())?;
    backup.set_value("OriginalName", &item.name).map_err(|e| e.to_string())?;
    backup.set_value("Command", &command).map_err(|e| e.to_string())?;
    backup.set_value("Hive", &item.hive).map_err(|e| e.to_string())?;
    backup.set_value("SourceKey", &item.key).map_err(|e| e.to_string())?;

    source.delete_value(&item.name).map_err(|_| access_hint(&item.hive))?;
    Ok(())
}

fn access_hint(hive: &str) -> String {
    if hive.eq_ignore_ascii_case("HKLM") {
        "Changing an all-users startup entry needs administrator rights.".into()
    } else {
        "Windows refused the change.".into()
    }
}

pub fn delete_startup_item(item: &StartupItem) -> Result<String, String> {
    if item.kind == StartupKind::Folder {
        let backup_dir = crate::core::store::data_dir().join("startup-backup");
        std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
        let target = PathBuf::from(&item.command);
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "shortcut.lnk".into());
        let dest = backup_dir.join(name);
        std::fs::rename(&target, &dest)
            .map_err(|_| "Could not move that shortcut (all-users entries need administrator rights).".to_string())?;
        return Ok(format!("Moved to {}", dest.display()));
    }

    let key = hive_root(&item.hive)
        .open_subkey_with_flags(&item.key, KEY_WRITE)
        .map_err(|_| access_hint(&item.hive))?;
    key.delete_value(&item.name).map_err(|_| access_hint(&item.hive))?;
    Ok(String::new())
}

/* ------------------------------------------------------ installed software */

pub fn installed_software() -> Vec<AppEntry> {
    const ROOTS: [(&str, &str); 3] = [
        ("HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        ("HKLM", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        ("HKCU", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];

    let mut out = Vec::new();
    for (hive, path) in ROOTS {
        let Ok(root) = hive_root(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
        for name in root.enum_keys().flatten() {
            let Ok(entry) = root.open_subkey_with_flags(&name, KEY_READ) else { continue };
            let display: String = entry.get_value("DisplayName").unwrap_or_default();
            if display.trim().is_empty() {
                continue;
            }
            let system_component: u32 = entry.get_value("SystemComponent").unwrap_or(0);
            if system_component == 1 {
                continue;
            }
            let parent: String = entry.get_value("ParentKeyName").unwrap_or_default();
            if !parent.is_empty() && !parent.to_ascii_lowercase().contains("uninstall") {
                continue;
            }
            let size_kb: u64 = entry.get_value("EstimatedSize").unwrap_or(0);
            out.push(AppEntry {
                id: format!("{}\\{}", hive, name),
                name: display.trim().to_string(),
                version: entry.get_value("DisplayVersion").unwrap_or_default(),
                publisher: entry.get_value("Publisher").unwrap_or_default(),
                install_date: format_reg_date(entry.get_value("InstallDate").unwrap_or_default()),
                size: size_kb * 1024,
                uninstall: entry.get_value("UninstallString").unwrap_or_default(),
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn format_reg_date(raw: String) -> String {
    let s = raw.trim().to_string();
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        return format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8]);
    }
    s
}

pub fn launch_uninstaller(command: &str) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("This entry has no uninstall command.".into());
    }
    Command::new("cmd.exe")
        .args(["/c", command])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/* ------------------------------------------------------------- processes */

#[cfg(windows)]
mod procs {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    fn filetime_100ns(ft: &FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
    }

    pub struct RawProc {
        pub pid: u32,
        pub name: String,
        pub threads: u32,
        pub memory: u64,
        pub cpu_100ns: u64,
    }

    pub fn snapshot() -> Vec<RawProc> {
        let mut out = Vec::new();
        unsafe {
            let snap: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    let name = String::from_utf16_lossy(
                        &entry.szExeFile[..entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len())],
                    );
                    let pid = entry.th32ProcessID;

                    let mut memory = 0u64;
                    let mut cpu = 0u64;
                    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if !handle.is_null() {
                        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                        if K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) != 0 {
                            memory = counters.WorkingSetSize as u64;
                        }
                        let (mut created, mut exited, mut kernel, mut user) = (
                            std::mem::zeroed::<FILETIME>(),
                            std::mem::zeroed::<FILETIME>(),
                            std::mem::zeroed::<FILETIME>(),
                            std::mem::zeroed::<FILETIME>(),
                        );
                        if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                            cpu = filetime_100ns(&kernel) + filetime_100ns(&user);
                        }
                        CloseHandle(handle);
                    }

                    if pid > 4 && !name.is_empty() {
                        out.push(RawProc { pid, name, threads: entry.cntThreads, memory, cpu_100ns: cpu });
                    }

                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        out
    }
}

pub fn processes() -> Vec<ProcRow> {
    let first = procs::snapshot();
    let started = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let second = procs::snapshot();
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;

    let mut by_pid: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    for p in &first {
        by_pid.insert(p.pid, p.cpu_100ns);
    }

    let mut rows: Vec<ProcRow> = second
        .into_iter()
        .map(|p| {
            let prev = by_pid.get(&p.pid).copied().unwrap_or(p.cpu_100ns);
            let delta = p.cpu_100ns.saturating_sub(prev) as f64 / 10_000_000.0;
            let pct = ((delta / elapsed) / cores * 100.0).clamp(0.0, 100.0) as f32;
            ProcRow {
                pid: p.pid,
                name: p.name,
                path: String::new(),
                memory: p.memory,
                cpu_pct: pct,
                threads: p.threads,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        b.cpu_pct
            .partial_cmp(&a.cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.memory.cmp(&a.memory))
    });
    rows
}

/// Ending any of these stops or crashes Windows, so the app never does it,
/// whatever the UI asks for.
pub fn is_critical_process(name: &str) -> bool {
    const CRITICAL: &[&str] = &[
        "system",
        "system idle process",
        "registry",
        "memory compression",
        "secure system",
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "winlogon.exe",
        "services.exe",
        "lsass.exe",
        "svchost.exe",
        "fontdrvhost.exe",
    ];
    CRITICAL.iter().any(|c| name.eq_ignore_ascii_case(c))
}

pub fn kill_process(pid: u32) -> Result<(), String> {
    if pid <= 4 {
        return Err("Invalid process id.".into());
    }
    // Resolve the name again here: ending a core Windows process can crash the
    // machine, so the guard lives with the action, not only in the UI.
    let name = processes()
        .into_iter()
        .find(|p| p.pid == pid)
        .map(|p| p.name)
        .unwrap_or_default();
    if is_critical_process(&name) {
        return Err(format!(
            "{} is a core Windows process. Ending it can crash this PC, so G-OpenCleaner will not.",
            name
        ));
    }
    let script = format!("Stop-Process -Id {} -Force -ErrorAction Stop; 'ok'", pid);
    ps(&script)?;
    Ok(())
}

/* --------------------------------------------------------------- drivers */

pub fn drivers() -> Result<Vec<DriverRow>, String> {
    let script = r#"
$rows = @()
foreach ($d in Get-CimInstance Win32_PnPSignedDriver -ErrorAction SilentlyContinue) {
  if (-not $d.DeviceName -or -not $d.DriverVersion) { continue }
  $rows += [pscustomobject]@{
    name = $d.DeviceName
    version = $d.DriverVersion
    date = if ($d.DriverDate) { $d.DriverDate.ToString('yyyy-MM-dd') } else { '' }
    manufacturer = $d.Manufacturer
    class = $d.DeviceClass
    hardware = @($d.HardwareID)
  }
}
$rows | ConvertTo-Json -Depth 3 -Compress
"#;
    let v = ps_json(script)?;
    let now = crate::core::store::now_secs();
    let cutoff = now.saturating_sub(3 * 365 * 24 * 3600);

    let mut rows: Vec<DriverRow> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in as_array(&v) {
        let name = str_of(item, "name").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let version = str_of(item, "version");
        if !seen.insert(format!("{}|{}", name, version)) {
            continue;
        }
        let date = str_of(item, "date");
        let old = !date.is_empty() && date_to_secs(&date).map(|t| t < cutoff).unwrap_or(false);
        let hardware_ids: Vec<String> = match item.get("hardware") {
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        };
        rows.push(DriverRow {
            name,
            class: str_of(item, "class"),
            version,
            date,
            manufacturer: str_of(item, "manufacturer"),
            old,
            hardware_ids,
        });
    }
    rows.sort_by(|a, b| a.class.cmp(&b.class).then(a.name.cmp(&b.name)));
    Ok(rows)
}

/// Read-only Windows Update Agent search for driver updates this PC does not
/// have yet. Nothing is downloaded or installed; the result is only a list.
pub fn driver_updates() -> Result<Vec<WuaUpdate>, String> {
    let script = r#"
$out = @()
try {
  $session = New-Object -ComObject Microsoft.Update.Session
  $session.ClientApplicationID = 'G-OpenCleaner'
  $searcher = $session.CreateUpdateSearcher()
  # 2 = the Windows Update service, not a corporate WSUS server.
  $searcher.ServerSelection = 2
  $result = $searcher.Search("IsInstalled=0 and Type='Driver' and IsHidden=0")
  foreach ($u in $result.Updates) {
    $out += [pscustomobject]@{
      title = [string]$u.Title
      hardware = [string]$u.DriverHardwareID
      class = [string]$u.DriverClass
      manufacturer = [string]$u.DriverManufacturer
      model = [string]$u.DriverModel
      date = if ($u.DriverVerDate) { ([datetime]$u.DriverVerDate).ToString('yyyy-MM-dd') } else { '' }
    }
  }
  [pscustomobject]@{ ok = $true; error = ''; updates = @($out) } | ConvertTo-Json -Depth 4 -Compress
} catch {
  [pscustomobject]@{ ok = $false; error = $_.Exception.Message; updates = @() } | ConvertTo-Json -Depth 4 -Compress
}
"#;

    let v = ps_json_timeout(script, DRIVER_SEARCH_TIMEOUT)?;
    if !v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
        let error = str_of(&v, "error");
        return Err(if error.is_empty() {
            "Windows Update did not answer the search.".into()
        } else {
            error
        });
    }

    let mut updates: Vec<WuaUpdate> = Vec::new();
    let list = v.get("updates").cloned().unwrap_or(Value::Null);
    for item in as_array(&list) {
        let hardware_id = str_of(item, "hardware").trim().to_string();
        let title = str_of(item, "title").trim().to_string();
        if title.is_empty() {
            continue;
        }
        if updates
            .iter()
            .any(|u| u.title == title && u.hardware_id == hardware_id)
        {
            continue;
        }
        updates.push(WuaUpdate {
            title,
            hardware_id,
            class: str_of(item, "class").trim().to_string(),
            manufacturer: str_of(item, "manufacturer").trim().to_string(),
            model: str_of(item, "model").trim().to_string(),
            date: str_of(item, "date").trim().to_string(),
        });
    }
    Ok(updates)
}

/// Opens the Windows Update page so Windows can install the driver, which is
/// the only thing G-OpenCleaner will do about it.
pub fn open_windows_update() -> Result<(), String> {
    Command::new("cmd.exe")
        .args(["/C", "start", "", "ms-settings:windowsupdate"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open Windows Update: {}", e))
}

fn date_to_secs(ymd: &str) -> Option<u64> {
    let mut it = ymd.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    // Days since epoch (civil-from-days algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days.max(0) as u64) * 86_400)
}

/* ---------------------------------------------------------------- winget */

pub struct WingetReport {
    pub supported: bool,
    pub items: Vec<UpgradeRow>,
}

pub fn winget_upgrades() -> WingetReport {
    let output = Command::new("winget.exe")
        .args([
            "list",
            "--upgrade-available",
            "--include-unknown",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let Ok(out) = output else {
        return WingetReport { supported: false, items: Vec::new() };
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() {
        return WingetReport { supported: false, items: Vec::new() };
    }

    let lines: Vec<&str> = text.lines().collect();
    let header = lines
        .iter()
        .position(|l| l.contains("Name") && l.contains("Id") && l.contains("Version"));

    let Some(header_idx) = header else {
        return WingetReport { supported: true, items: Vec::new() };
    };
    let h = lines[header_idx];
    let Some(id_col) = h.find("Id") else {
        return WingetReport { supported: true, items: Vec::new() };
    };
    let Some(ver_col) = h.find("Version") else {
        return WingetReport { supported: true, items: Vec::new() };
    };
    let Some(avail_col) = h.find("Available") else {
        return WingetReport { supported: true, items: Vec::new() };
    };

    let mut items = Vec::new();
    for line in lines.iter().skip(header_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.chars().all(|c| c == '-') {
            continue;
        }
        if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && trimmed.contains("upgrade") {
            continue;
        }
        if line.len() <= id_col {
            continue;
        }
        let name = line[..id_col].trim().to_string();
        let id = line[id_col..ver_col.min(line.len())].trim().to_string();
        let version = if ver_col < line.len() {
            line[ver_col..avail_col.min(line.len())].trim().to_string()
        } else {
            String::new()
        };
        let available = if avail_col < line.len() {
            line[avail_col..].split_whitespace().next().unwrap_or("").to_string()
        } else {
            String::new()
        };
        if name.is_empty() || id.is_empty() {
            continue;
        }
        items.push(UpgradeRow { name, id, version, available });
    }
    WingetReport { supported: true, items }
}

pub fn winget_upgrade(id: &str) -> Result<String, String> {
    let out = Command::new("winget.exe")
        .args([
            "upgrade",
            "--id",
            id,
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;

    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let tail: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let summary = tail
        .iter()
        .rev()
        .take(3)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    if !out.status.success() {
        return Err(if summary.is_empty() { "winget could not complete the upgrade.".into() } else { summary });
    }
    Ok(summary)
}

/* ----------------------------------------------------------------- tasks */

pub fn scheduled_tasks() -> Vec<TaskRow> {
    let script = r#"
$rows = @()
foreach ($t in Get-ScheduledTask -ErrorAction SilentlyContinue) {
  if ($t.TaskPath -like '\Microsoft\*') { continue }
  $rows += [pscustomobject]@{
    name = $t.TaskName
    path = $t.TaskPath
    state = [string]$t.State
  }
}
$rows | ConvertTo-Json -Depth 3 -Compress
"#;
    let Ok(v) = ps_json(script) else { return Vec::new() };
    as_array(&v)
        .into_iter()
        .map(|t| TaskRow {
            name: str_of(t, "name"),
            path: str_of(t, "path"),
            state: str_of(t, "state"),
        })
        .filter(|t| !t.name.is_empty())
        .collect()
}

/* --------------------------------------------------------------- actions */

pub fn run_action(action: Action) -> Result<String, String> {
    match action {
        Action::Dns => {
            let out = Command::new("ipconfig.exe")
                .arg("/flushdns")
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err("ipconfig /flushdns failed.".into());
            }
            Ok(Action::Dns.label().to_string())
        }
        Action::Clipboard => {
            clipboard_clear()?;
            Ok(Action::Clipboard.label().to_string())
        }
        Action::RecycleBin => {
            empty_recycle_bin()?;
            Ok(Action::RecycleBin.label().to_string())
        }
    }
}

fn clipboard_clear() -> Result<(), String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("Could not open the clipboard.".into());
        }
        let ok = EmptyClipboard();
        CloseClipboard();
        if ok == 0 {
            return Err("Could not clear the clipboard.".into());
        }
    }
    Ok(())
}

pub fn empty_recycle_bin() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::SHEmptyRecycleBinW;
    const SHERB_NOCONFIRMATION: u32 = 0x0000_0001;
    const SHERB_NOPROGRESSUI: u32 = 0x0000_0002;
    const SHERB_NOSOUND: u32 = 0x0000_0004;

    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    if hr < 0 {
        return Err("Windows could not empty the Recycle Bin.".into());
    }
    Ok(())
}

pub fn restore_point(label: &str) -> Result<(), String> {
    let script = format!(
        "Checkpoint-Computer -Description '{}' -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop; 'ok'",
        label.replace('\'', "''")
    );
    match ps(&script) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.to_lowercase().contains("access") || e.to_lowercase().contains("denied") {
                Err("Creating a restore point needs administrator rights.".into())
            } else if e.to_lowercase().contains("disabled") || e.to_lowercase().contains("frequency") {
                Err("System Restore is disabled or throttled on this PC.".into())
            } else if e.is_empty() {
                Err("Could not create a restore point.".into())
            } else {
                Err(e)
            }
        }
    }
}

pub fn open_in_explorer(path: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn is_admin() -> bool {
    std::env::var("OPENCLEAN_ADMIN").is_ok()
}

pub fn home_dir() -> String {
    std::env::var("USERPROFILE").unwrap_or_default()
}

pub fn app_data_dir() -> String {
    std::env::var("APPDATA").unwrap_or_default()
}

pub fn local_app_data_dir() -> String {
    std::env::var("LOCALAPPDATA").unwrap_or_default()
}

pub fn program_data_dir() -> String {
    std::env::var("ProgramData").unwrap_or_default()
}

pub fn windows_dir() -> String {
    std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into())
}

/// Starts this executable again through the shell's `runas` verb — that is what
/// raises the UAC prompt — and waits for it. Returns the helper's exit code;
/// `Err` when the prompt was dismissed.
pub fn elevate(exe: &str, args: &str) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let verb = wide("runas");
    let file = wide(exe);
    let parameters = wide(args);

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = 1; // SW_SHOWNORMAL

    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(0);
        return Err(if code == 1223 {
            "The Windows approval prompt was cancelled, so nothing was changed.".into()
        } else {
            format!("Windows refused to elevate the helper (error {}).", code)
        });
    }

    let mut exit = 0u32;
    unsafe {
        if !info.hProcess.is_null() {
            WaitForSingleObject(info.hProcess, u32::MAX);
            GetExitCodeProcess(info.hProcess, &mut exit);
            CloseHandle(info.hProcess);
        }
    }
    Ok(exit)
}

pub fn spawn_detached(program: &str, args: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
