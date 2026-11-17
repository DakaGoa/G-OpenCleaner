//! Scanner and cleaner.
//!
//! Safety model (identical to the web build, enforced here in one place):
//!   * protected system locations are refused before anything else, whatever
//!     allowlist or caller-supplied root applies (`is_protected`)
//!   * deletion is only permitted strictly inside a named disposable-cache root
//!   * a caller-supplied root is only honoured when it is ordinary user data,
//!     never a system tree (`is_safe_extra_root`)
//!   * a root folder itself is never deleted (only its children)
//!   * reparse points (junctions / symlinks) are never traversed or removed
//!   * exclusions short-circuit both scanning and deletion

use crate::core::model::{Action, CleanOutcome, Mode, Rule, RulePreview, ScanResult, Target};
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 250_000;
const MAX_MS_PER_RULE: u128 = 25_000;

#[derive(Clone, Default)]
pub struct CleanOpts {
    pub secure: bool,
    pub exclusions: Vec<String>,
    pub extra_roots: Vec<PathBuf>,
}

#[derive(Default)]
pub struct RemoveStats {
    pub removed: u64,
    pub bytes: u64,
    pub skipped: u64,
    /// Part of `bytes` that was moved to quarantine rather than destroyed.
    pub quarantined: u64,
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// Lowercased, absolute-ish path used for every comparison.
pub fn norm(p: &Path) -> String {
    let canon = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = canon.to_string_lossy().to_string();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
    s.trim_end_matches(['\\', '/']).to_ascii_lowercase()
}

fn inside(child: &str, parent: &str) -> bool {
    if parent.is_empty() || child == parent {
        return false;
    }
    child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'\\')
}

fn is_drive_root(s: &str) -> bool {
    s.len() <= 2 && s.ends_with(':')
}

fn windir() -> String {
    let w = env("windir");
    if w.is_empty() { r"C:\Windows".into() } else { w }
}

pub fn allowed_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in [
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
        "APPDATA",
        "ProgramData",
    ] {
        let v = env(var);
        if !v.is_empty() {
            roots.push(PathBuf::from(v));
        }
    }
    if !env("ProgramData").is_empty() {
        roots.push(PathBuf::from(env("ProgramData")).join(r"Microsoft\Windows"));
    }

    let w = PathBuf::from(windir());
    for sub in [
        "Temp",
        "Logs",
        "Prefetch",
        "Panther",
        "Minidump",
        "LiveKernelReports",
        r"SoftwareDistribution\Download",
        r"SoftwareDistribution\DeliveryOptimization",
        // Only the font cache, not the whole LocalService profile.
        r"ServiceProfiles\LocalService\AppData\Local\FontCache",
    ] {
        roots.push(w.join(sub));
    }
    roots
}

pub fn allowed_files() -> Vec<PathBuf> {
    vec![PathBuf::from(windir()).join("MEMORY.DMP")]
}

/* ------------------------------------------------------------ protection */

/// The user's personal folders, wherever Windows actually points them, so a
/// OneDrive redirect is covered too. Nothing is ever deleted from here.
pub fn personal_roots() -> Vec<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |raw: String| {
        let expanded = expand_vars(&raw).unwrap_or(raw);
        let trimmed = expanded.trim().to_string();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    };

    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
    {
        for name in ["Personal", "Desktop", "My Pictures", "My Video", "My Music"] {
            if let Ok(value) = key.get_value::<String, _>(name) {
                push(value);
            }
        }
    }

    // Always include the defaults as well: a missing registry value must not
    // leave a folder unprotected.
    let home = env("USERPROFILE");
    if !home.is_empty() {
        for sub in ["Documents", "Desktop", "Pictures", "Videos", "Music"] {
            out.push(PathBuf::from(&home).join(sub));
        }
    }

    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase()));
    out
}

/// Locations no deletion may ever reach, whatever allowlist or extra root
/// applies. These are the parts of Windows that, once missing, stop a PC
/// booting, logging in, verifying drivers or installing updates — plus the
/// user's own document, desktop and media folders.
pub fn protected_roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let win = PathBuf::from(windir());
    for sub in [
        "System32",
        "SysWOW64",
        "WinSxS",
        "servicing",
        "assembly",
        "Boot",
        "Fonts",
        "CSC",
        r"System32\config",
        r"System32\drivers",
        r"System32\catroot",
        r"System32\DriverStore",
        r"System32\spool",
        r"System32\LogFiles",
    ] {
        out.push(win.join(sub));
    }

    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        let v = env(var);
        if !v.is_empty() {
            out.push(PathBuf::from(v));
        }
    }

    let program_data = env("ProgramData");
    if !program_data.is_empty() {
        let base = PathBuf::from(program_data);
        for sub in [
            r"Microsoft\Crypto",
            r"Microsoft\Protect",
            r"Microsoft\SystemCertificates",
            r"Microsoft\Windows\SystemData",
            r"Microsoft\Windows Defender",
            r"Microsoft\Windows\Start Menu",
            "Package Cache",
            "ssh",
        ] {
            out.push(base.join(sub));
        }
    }

    for var in ["APPDATA", "LOCALAPPDATA"] {
        let v = env(var);
        if v.is_empty() {
            continue;
        }
        let base = PathBuf::from(v);
        for sub in [
            r"Microsoft\Crypto",
            r"Microsoft\Protect",
            r"Microsoft\SystemCertificates",
            r"Microsoft\Windows\Start Menu",
        ] {
            out.push(base.join(sub));
        }
    }

    // Per-user program installations, G-OpenCleaner included.
    let local = env("LOCALAPPDATA");
    if !local.is_empty() {
        out.push(PathBuf::from(local).join("Programs"));
    }

    // Personal folders: never bulk-deleted, even from a folder the user chose
    // in the Disk Analyzer or Duplicate Finder.
    out.extend(personal_roots());

    // G-OpenCleaner's own settings, statistics and self-test report.
    out.push(crate::core::store::data_dir());
    out
}

/// File names that are never junk, wherever they are found.
const PROTECTED_NAMES: &[&str] = &[
    "ntoskrnl.exe",
    "ntkrnlmp.exe",
    "winload.exe",
    "winload.efi",
    "winresume.exe",
    "winresume.efi",
    "bootmgr",
    "bootnxt",
    "bcd",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "ntuser.dat",
    "ntuser.ini",
    "system",
    "sam",
    "security",
    "software",
    "default",
];

/// File types that are never junk.
const PROTECTED_EXTS: &[&str] = &[".sys", ".efi", ".drv"];

fn norm_protected() -> &'static Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        protected_roots()
            .iter()
            .map(|p| norm(p))
            .filter(|s| !s.is_empty())
            .collect()
    })
}

fn norm_allowed() -> &'static Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        allowed_roots()
            .iter()
            .map(|p| norm(p))
            .filter(|s| !s.is_empty())
            .collect()
    })
}

/// True when a path must never be removed, for any reason, by any caller.
pub fn is_protected(target: &Path) -> bool {
    let t = norm(target);
    if t.is_empty() {
        return true;
    }
    let name = t.rsplit('\\').next().unwrap_or("");
    if PROTECTED_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
        return true;
    }
    if PROTECTED_EXTS.iter().any(|ext| t.ends_with(ext)) {
        return true;
    }
    for root in norm_protected() {
        if t == *root || inside(&t, root) {
            return true;
        }
    }
    false
}

/// A caller-supplied root (disk analyzer, duplicate finder, cloud cleaner) is
/// only trusted when it is ordinary user data rather than a system tree.
pub fn is_safe_extra_root(root: &Path) -> bool {
    let r = norm(root);
    if r.is_empty() || is_drive_root(&r) {
        return false;
    }
    if is_protected(Path::new(&r)) {
        return false;
    }
    let win = norm(Path::new(&windir()));
    if !win.is_empty() && (r == win || inside(&r, &win)) {
        return false;
    }
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"] {
        let v = env(var);
        if v.is_empty() {
            continue;
        }
        let p = norm(Path::new(&v));
        if !p.is_empty() && (r == p || inside(&r, &p)) {
            return false;
        }
    }
    true
}

/// Files/directories that may be removed. `%WINDIR%` itself is deliberately
/// absent, so no rule can ever reach System32.
pub fn is_safe_to_delete(target: &Path, extra_roots: &[PathBuf]) -> bool {
    let t = norm(target);
    if t.is_empty() || is_drive_root(&t) {
        return false;
    }
    // Nothing inside a protected system location, ever.
    if is_protected(Path::new(&t)) {
        return false;
    }
    if allowed_files().iter().any(|f| norm(f) == t) {
        return true;
    }
    for root in extra_roots {
        if is_safe_extra_root(root) && inside(&t, &norm(root)) {
            return true;
        }
    }
    for root in norm_allowed() {
        if inside(&t, root) {
            return true;
        }
    }
    false
}

/// A folder may be entered when it equals or sits inside an allowed root, and
/// is never a protected system location.
pub fn is_safe_container(target: &Path, extra_roots: &[PathBuf]) -> bool {
    let t = norm(target);
    if t.is_empty() || is_drive_root(&t) {
        return false;
    }
    if is_protected(Path::new(&t)) {
        return false;
    }
    for root in norm_allowed() {
        if t == *root || inside(&t, root) {
            return true;
        }
    }
    for root in extra_roots {
        if !is_safe_extra_root(root) {
            continue;
        }
        let r = norm(root);
        if t == r || inside(&t, &r) {
            return true;
        }
    }
    false
}

pub fn is_excluded(target: &Path, exclusions: &[String]) -> bool {
    if exclusions.is_empty() {
        return false;
    }
    let t = norm(target);
    exclusions.iter().any(|raw| {
        if raw.trim().is_empty() {
            return false;
        }
        let e = norm(Path::new(raw));
        !e.is_empty() && (t == e || inside(&t, &e))
    })
}

/// Expands %VARS%. Returns None when a variable cannot be resolved, so the
/// scanner skips that target instead of guessing a path.
pub fn expand_vars(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                let value = env(&name);
                if value.is_empty() {
                    return None;
                }
                out.push_str(&value);
                i += end + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    Some(out.replace('/', r"\"))
}

/// Expands `*` path segments (browser profiles, package folders).
pub fn expand_glob(input: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = input.split('\\').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() || !parts[0].ends_with(':') {
        return Vec::new();
    }
    let mut current = vec![PathBuf::from(format!("{}\\", parts[0]))];

    for seg in &parts[1..] {
        let mut next: Vec<PathBuf> = Vec::new();
        for base in &current {
            if !seg.contains('*') {
                next.push(base.join(seg));
                continue;
            }
            let mut matches = 0;
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if wildcard_match(&name, seg) {
                        next.push(entry.path());
                        matches += 1;
                        if matches >= 64 {
                            break;
                        }
                    }
                }
            }
        }
        current = next;
        if current.is_empty() {
            return Vec::new();
        }
    }
    current
}

pub fn wildcard_match(name: &str, pattern: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let p = pattern.to_ascii_lowercase();
    if p == "*" {
        return true;
    }
    // Simple glob: split on '*' and match the pieces in order.
    let mut rest = n.as_str();
    let mut first = true;
    for piece in p.split('*') {
        if piece.is_empty() {
            first = false;
            continue;
        }
        if first {
            if !rest.starts_with(piece) {
                return false;
            }
            rest = &rest[piece.len()..];
            first = false;
        } else if let Some(pos) = rest.find(piece) {
            rest = &rest[pos + piece.len()..];
        } else {
            return false;
        }
    }
    if p.ends_with('*') {
        true
    } else {
        rest.is_empty()
    }
}

#[derive(Clone)]
pub struct Resolved {
    pub mode: Mode,
    pub paths: Vec<PathBuf>,
    pub pattern: Option<&'static str>,
    pub older_than_hours: Option<u64>,
    pub action: Option<Action>,
}

pub fn resolve(rule: &Rule) -> Vec<Resolved> {
    let mut out = Vec::new();
    for target in &rule.targets {
        if target.mode == Mode::Action {
            out.push(Resolved {
                mode: Mode::Action,
                paths: Vec::new(),
                pattern: None,
                older_than_hours: None,
                action: target.action,
            });
            continue;
        }
        let Some(expanded) = expand_vars(&target.path) else {
            continue;
        };
        let paths = if expanded.contains('*') {
            expand_glob(&expanded)
        } else if Path::new(&expanded).exists() {
            vec![PathBuf::from(&expanded)]
        } else {
            Vec::new()
        };
        if paths.is_empty() {
            continue;
        }
        out.push(Resolved {
            mode: target.mode,
            paths,
            pattern: target.pattern,
            older_than_hours: target.older_than_hours,
            action: None,
        });
    }
    out
}

pub fn is_applicable(rule: &Rule) -> bool {
    !resolve(rule).is_empty()
}

/* ------------------------------------------------------------------ walk */

struct Walked {
    files: Vec<(PathBuf, u64, u64)>,
    dirs: Vec<PathBuf>,
    truncated: bool,
}

fn walk(dir: &Path, pattern: Option<&str>, older_than_hours: Option<u64>, deadline: Instant) -> Walked {
    let mut out = Walked { files: Vec::new(), dirs: Vec::new(), truncated: false };
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    let now = now_millis();

    while let Some(current) = stack.pop() {
        if Instant::now() > deadline || seen > MAX_ENTRIES {
            out.truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                out.truncated = true;
                break;
            }
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let full = entry.path();
            if ft.is_dir() {
                out.dirs.push(full.clone());
                stack.push(full);
            } else if ft.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(pat) = pattern {
                    if !wildcard_match(&name, pat) {
                        continue;
                    }
                }
                let Ok(meta) = entry.metadata() else { continue };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                if let Some(hours) = older_than_hours {
                    let age_hours = (now.saturating_sub(mtime)) as f64 / 3_600_000.0;
                    if age_hours < hours as f64 {
                        continue;
                    }
                }
                out.files.push((full, meta.len(), mtime));
            }
        }
    }
    out
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn size_of(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(path) else { return (0, 0) };
    if !meta.is_dir() {
        return (meta.len(), 1);
    }
    let walked = walk(path, None, None, Instant::now() + std::time::Duration::from_secs(20));
    let bytes = walked.files.iter().map(|f| f.1).sum();
    (bytes, walked.files.len() as u64)
}

/* ------------------------------------------------------------------ scan */

pub fn scan_rule(rule: &Rule, opts: &CleanOpts) -> ScanResult {
    let started = Instant::now();
    let deadline = started + std::time::Duration::from_millis(MAX_MS_PER_RULE as u64);
    let mut result = ScanResult {
        id: rule.id.clone(),
        name: rule.name.clone(),
        ..Default::default()
    };

    for target in resolve(rule) {
        if started.elapsed().as_millis() > MAX_MS_PER_RULE * 2 {
            result.truncated = true;
            break;
        }
        if target.mode == Mode::Action {
            if let Some(action) = target.action {
                result.actions.push(action);
            }
            continue;
        }

        for path in &target.paths {
            let safe = if target.mode == Mode::File {
                is_safe_to_delete(path, &opts.extra_roots)
            } else {
                is_safe_container(path, &opts.extra_roots)
            };
            if !safe {
                result.get_or_insert_error("Skipped a location outside the allowed cache roots.");
                continue;
            }

            match target.mode {
                Mode::File => {
                    if is_excluded(path, &opts.exclusions) {
                        continue;
                    }
                    let Ok(meta) = fs::symlink_metadata(path) else { continue };
                    if meta.is_dir() {
                        continue;
                    }
                    result.size += meta.len();
                    result.count += 1;
                    if result.samples.len() < 24 {
                        result.samples.push((path.to_string_lossy().to_string(), meta.len()));
                    }
                }
                Mode::Contents => {
                    let Ok(entries) = fs::read_dir(path) else { continue };
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
                            continue;
                        }
                        let full = entry.path();
                        if is_excluded(&full, &opts.exclusions) {
                            continue;
                        }
                        let (bytes, files) = size_of(&full);
                        result.size += bytes;
                        result.count += files;
                        if result.samples.len() < 24 {
                            result.samples.push((full.to_string_lossy().to_string(), bytes));
                        }
                    }
                }
                Mode::Recurse => {
                    let walked = walk(path, target.pattern, target.older_than_hours, deadline);
                    for (file, size, _) in walked.files {
                        if is_excluded(&file, &opts.exclusions) {
                            continue;
                        }
                        result.size += size;
                        result.count += 1;
                        if result.samples.len() < 24 {
                            result.samples.push((file.to_string_lossy().to_string(), size));
                        }
                    }
                    if walked.truncated {
                        result.truncated = true;
                    }
                }
                Mode::Action => {}
            }
        }
    }
    result
}

/* ---------------------------------------------------------------- preview */

/// Keeps the `limit` biggest files seen so far without holding the rest.
struct TopN {
    limit: usize,
    items: Vec<(String, u64)>,
}

impl TopN {
    fn new(limit: usize) -> Self {
        TopN {
            limit: limit.max(1),
            items: Vec::new(),
        }
    }

    fn push(&mut self, path: &Path, size: u64) {
        if self.items.len() < self.limit {
            self.items.push((path.to_string_lossy().to_string(), size));
            return;
        }
        let smallest = self
            .items
            .iter()
            .enumerate()
            .min_by_key(|(_, (_, s))| *s)
            .map(|(i, _)| i);
        if let Some(idx) = smallest {
            if size > self.items[idx].1 {
                self.items[idx] = (path.to_string_lossy().to_string(), size);
            }
        }
    }

    fn into_sorted(mut self) -> Vec<(String, u64)> {
        self.items
            .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        self.items
    }
}

fn preview_tree(
    path: &Path,
    opts: &CleanOpts,
    out: &mut RulePreview,
    top: &mut TopN,
    deadline: Instant,
) {
    if Instant::now() > deadline {
        out.truncated = true;
        return;
    }
    let Ok(meta) = fs::symlink_metadata(path) else { return };
    if meta.file_type().is_symlink() {
        return;
    }
    if meta.is_dir() {
        if !is_safe_container(path, &opts.extra_roots) {
            return;
        }
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                preview_tree(&entry.path(), opts, out, top, deadline);
            }
        }
        return;
    }
    if is_excluded(path, &opts.exclusions) || !is_safe_to_delete(path, &opts.extra_roots) {
        return;
    }
    out.total += meta.len();
    out.files += 1;
    top.push(path, meta.len());
}

/// What a rule would delete, read-only: the largest `limit` paths and the real
/// totals. Bounded by a time budget so expanding a row stays quick even for a
/// category with hundreds of thousands of files.
pub fn preview_rule(rule: &Rule, opts: &CleanOpts, limit: usize) -> RulePreview {
    let mut out = RulePreview {
        id: rule.id.clone(),
        ..Default::default()
    };
    let mut top = TopN::new(limit);
    let deadline = Instant::now() + std::time::Duration::from_secs(6);

    for target in resolve(rule) {
        if Instant::now() > deadline {
            out.truncated = true;
            break;
        }
        if target.mode == Mode::Action {
            continue;
        }
        for path in &target.paths {
            let safe = if target.mode == Mode::File {
                is_safe_to_delete(path, &opts.extra_roots)
            } else {
                is_safe_container(path, &opts.extra_roots)
            };
            if !safe {
                continue;
            }

            match target.mode {
                Mode::File => {
                    let Ok(meta) = fs::symlink_metadata(path) else { continue };
                    if meta.is_dir() || is_excluded(path, &opts.exclusions) {
                        continue;
                    }
                    out.total += meta.len();
                    out.files += 1;
                    top.push(path, meta.len());
                }
                Mode::Contents => {
                    if let Ok(entries) = fs::read_dir(path) {
                        for entry in entries.flatten() {
                            preview_tree(&entry.path(), opts, &mut out, &mut top, deadline);
                        }
                    }
                }
                Mode::Recurse => {
                    let walked = walk(path, target.pattern, target.older_than_hours, deadline);
                    for (file, size, _) in walked.files {
                        if is_excluded(&file, &opts.exclusions) {
                            continue;
                        }
                        out.total += size;
                        out.files += 1;
                        top.push(&file, size);
                    }
                    if walked.truncated {
                        out.truncated = true;
                    }
                }
                Mode::Action => {}
            }
        }
    }

    out.paths = top.into_sorted();
    out
}

impl ScanResult {
    fn get_or_insert_error(&mut self, msg: &str) {
        if self.error.is_none() {
            self.error = Some(msg.to_string());
        }
    }
}

/* ----------------------------------------------------------------- clean */

fn overwrite(path: &Path, size: u64) {
    let Ok(mut file) = fs::OpenOptions::new().read(true).write(true).open(path) else { return };
    let mut seed = 0x9E37_79B9_7F4A_7C15u64 ^ size;
    let mut block = [0u8; 65_536];
    for chunk in block.chunks_mut(8) {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let bytes = seed.to_le_bytes();
        for (i, b) in chunk.iter_mut().enumerate() {
            *b = bytes[i % 8];
        }
    }
    let limit = size.min(8 * 1024 * 1024);
    let mut written = 0u64;
    let _ = file.seek(SeekFrom::Start(0));
    while written < limit {
        let take = ((limit - written) as usize).min(block.len());
        if file.write_all(&block[..take]).is_err() {
            break;
        }
        written += take as u64;
    }
    let _ = file.sync_all();
}

/// Whether a rule's removals go to quarantine instead of being destroyed.
/// Permanent rules (the Recycle Bin), secure delete and files a user picked in
/// the analyzers are deleted outright.
pub fn quarantines(rule: &Rule, opts: &CleanOpts) -> bool {
    quarantines_flag(rule.permanent, opts)
}

/// The same decision, without needing a whole rule (used by the self-test).
pub fn quarantines_flag(permanent: bool, opts: &CleanOpts) -> bool {
    !permanent && !opts.secure
}

/// Removes a file or a directory tree. The top-level target is removed too when
/// it is a directory; callers that clear a cache folder pass its children.
///
/// With a `Quarantiner`, files are moved there instead of deleted. If a file
/// cannot be stored it is left untouched: a promised quarantine is never
/// silently turned into a deletion.
pub fn remove_path(
    path: &Path,
    opts: &CleanOpts,
    mut quarantine: Option<&mut crate::core::quarantine::Quarantiner>,
) -> RemoveStats {
    let mut stats = RemoveStats::default();
    if !is_safe_to_delete(path, &opts.extra_roots) {
        stats.skipped = 1;
        return stats;
    }
    if is_excluded(path, &opts.exclusions) {
        stats.skipped = 1;
        return stats;
    }
    let Ok(meta) = fs::symlink_metadata(path) else { return stats };

    if !meta.is_dir() {
        if let Some(q) = quarantine {
            if q.store(path).is_some() {
                stats.removed = 1;
                stats.bytes = meta.len();
                stats.quarantined = meta.len();
            } else {
                stats.skipped = 1;
            }
            return stats;
        }
        if opts.secure {
            overwrite(path, meta.len());
        }
        match fs::remove_file(path) {
            Ok(_) => {
                stats.removed = 1;
                stats.bytes = meta.len();
            }
            Err(_) => stats.skipped = 1,
        }
        return stats;
    }

    let Ok(entries) = fs::read_dir(path) else {
        stats.skipped = 1;
        return stats;
    };
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
            continue;
        }
        let child = entry.path();
        if is_excluded(&child, &opts.exclusions) {
            stats.skipped += 1;
            continue;
        }
        let inner = remove_path(&child, opts, quarantine.as_deref_mut());
        stats.removed += inner.removed;
        stats.bytes += inner.bytes;
        stats.skipped += inner.skipped;
        stats.quarantined += inner.quarantined;
        let _ = fs::remove_dir(&child); // no-op for files, empties nested folders
    }
    let _ = fs::remove_dir(path);
    stats
}

pub fn clean_rule(rule: &Rule, opts: &CleanOpts) -> CleanOutcome {
    let mut outcome = CleanOutcome {
        id: rule.id.clone(),
        name: rule.name.clone(),
        ..Default::default()
    };

    let mut quarantine = if quarantines(rule, opts) {
        crate::core::quarantine::Quarantiner::new(&rule.name)
    } else {
        None
    };

    for target in resolve(rule) {
        if target.mode == Mode::Action {
            if let Some(action) = target.action {
                match crate::core::win::run_action(action) {
                    Ok(label) => outcome.actions.push(label),
                    Err(e) => outcome.errors.push(e),
                }
            }
            continue;
        }

        for path in &target.paths {
            let safe = if target.mode == Mode::File {
                is_safe_to_delete(path, &opts.extra_roots)
            } else {
                is_safe_container(path, &opts.extra_roots)
            };
            if !safe {
                outcome.skipped += 1;
                continue;
            }

            match target.mode {
                Mode::File => {
                    let stats = remove_path(path, opts, quarantine.as_mut());
                    outcome.bytes += stats.bytes;
                    outcome.removed += stats.removed;
                    outcome.skipped += stats.skipped;
                    outcome.quarantined += stats.quarantined;
                }
                Mode::Contents => {
                    let Ok(entries) = fs::read_dir(path) else {
                        outcome.skipped += 1;
                        continue;
                    };
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
                            continue;
                        }
                        let stats = remove_path(&entry.path(), opts, quarantine.as_mut());
                        outcome.bytes += stats.bytes;
                        outcome.removed += stats.removed;
                        outcome.skipped += stats.skipped;
                        outcome.quarantined += stats.quarantined;
                    }
                }
                Mode::Recurse => {
                    let walked = walk(
                        path,
                        target.pattern,
                        target.older_than_hours,
                        Instant::now() + std::time::Duration::from_secs(30),
                    );
                    for (file, _, _) in walked.files {
                        let stats = remove_path(&file, opts, quarantine.as_mut());
                        outcome.bytes += stats.bytes;
                        outcome.removed += stats.removed;
                        outcome.skipped += stats.skipped;
                        outcome.quarantined += stats.quarantined;
                    }
                }
                Mode::Action => {}
            }
        }
    }

    if let Some(q) = quarantine.take() {
        outcome.quarantined = q.finish().min(outcome.bytes);
    }
    outcome
}

/// Targets of a rule, for the self-test that checks every path is allowed.
pub fn audit_rules(rules: &[Rule]) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for rule in rules {
        for target in resolve(rule) {
            for path in &target.paths {
                let ok = if target.mode == Mode::File {
                    is_safe_to_delete(path, &[])
                } else {
                    is_safe_container(path, &[])
                };
                out.push((rule.id.clone(), path.to_string_lossy().to_string(), ok));
            }
        }
    }
    out
}

pub fn target_count(rule: &Rule) -> usize {
    rule.targets
        .iter()
        .map(|t: &Target| if t.mode == Mode::Action { 1 } else { 1 })
        .count()
}
