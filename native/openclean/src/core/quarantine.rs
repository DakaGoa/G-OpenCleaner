//! Quarantine: removable files are moved here instead of being destroyed.
//!
//! Layout:
//!   %LOCALAPPDATA%\OpenClean\Quarantine\<run>\manifest.json
//!   %LOCALAPPDATA%\OpenClean\Quarantine\<run>\files\<drive>\<original path>
//!
//! A run is one cleaning operation. Entries keep their original path so a
//! restore puts the file back exactly where it came from. Runs older than
//! [`RETENTION_DAYS`] are purged automatically.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RETENTION_DAYS: u64 = 7;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    /// Where the file came from.
    pub original: String,
    /// Path inside the run directory.
    pub stored: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Run {
    pub id: String,
    pub at: u64,
    pub label: String,
    pub bytes: u64,
    pub entries: Vec<Entry>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The quarantine folder. `OPENCLEAN_QUARANTINE_DIR` overrides it, which the
/// self-test uses so it never touches a user's real quarantine.
pub fn root() -> PathBuf {
    if let Ok(custom) = std::env::var("OPENCLEAN_QUARANTINE_DIR") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom);
        }
    }
    crate::core::store::data_dir().join("Quarantine")
}

fn stamp() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "run-{}-{}-{}",
        now_secs(),
        std::process::id(),
        n
    )
}

/// Flattens an absolute path into a relative key such as `C\Users\me\a.tmp`.
fn relative_key(source: &Path) -> String {
    let raw = source.to_string_lossy().to_string();
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            ':' => {}
            '\\' | '/' => out.push('\\'),
            _ => out.push(ch),
        }
    }
    out.trim_start_matches('\\').to_string()
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)?;
    Ok(())
}

fn write_manifest(dir: &Path, run: &Run) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(run)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(dir.join("manifest.json"), json)
}

fn read_manifest(dir: &Path) -> Option<Run> {
    let raw = fs::read_to_string(dir.join("manifest.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

/// A valid run directory inside the quarantine root, or `None`.
fn run_dir(id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains('\\') || id.contains('/') || id.contains("..") {
        return None;
    }
    let base = root();
    let dir = base.join(id);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/* ------------------------------------------------------------- collecting */

/// One cleaning operation's worth of moved files.
pub struct Quarantiner {
    dir: PathBuf,
    run: Run,
}

impl Quarantiner {
    pub fn new(label: &str) -> Option<Self> {
        let id = stamp();
        let dir = root().join(&id);
        fs::create_dir_all(&dir).ok()?;
        Some(Quarantiner {
            dir,
            run: Run {
                id,
                at: now_secs(),
                label: label.to_string(),
                bytes: 0,
                entries: Vec::new(),
            },
        })
    }

    pub fn id(&self) -> &str {
        &self.run.id
    }

    pub fn entries(&self) -> &[Entry] {
        &self.run.entries
    }

    /// Moves one file into quarantine. Returns its size, or `None` when it
    /// could not be stored — the caller must then leave the file alone.
    pub fn store(&mut self, source: &Path) -> Option<u64> {
        if crate::core::scanner::is_protected(source) {
            return None;
        }
        let meta = fs::symlink_metadata(source).ok()?;
        if meta.is_dir() || meta.file_type().is_symlink() {
            return None;
        }
        let size = meta.len();

        let mut dest = self.dir.join("files").join(relative_key(source));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok()?;
        }
        let mut bump = 1;
        while dest.exists() {
            let name = dest.file_name()?.to_string_lossy().to_string();
            let alt = format!("{}.{}", name, bump);
            dest.set_file_name(alt);
            bump += 1;
            if bump > 100 {
                return None;
            }
        }

        move_file(source, &dest).ok()?;

        let stored = dest
            .strip_prefix(&self.dir)
            .ok()?
            .to_string_lossy()
            .to_string();
        self.run.bytes += size;
        self.run.entries.push(Entry {
            original: source.to_string_lossy().to_string(),
            stored,
            size,
        });
        Some(size)
    }

    /// Writes the manifest and returns the bytes stored. An empty run leaves
    /// nothing behind.
    pub fn finish(self) -> u64 {
        if self.run.entries.is_empty() {
            let _ = fs::remove_dir_all(&self.dir);
            return 0;
        }
        if write_manifest(&self.dir, &self.run).is_err() {
            // Without a manifest nothing could be restored or listed, so put the
            // files back is impossible here; keep the folder and report zero so
            // the summary never claims a quarantine that cannot be undone.
            return 0;
        }
        self.run.bytes
    }
}

/* ------------------------------------------------------------------- query */

pub fn list() -> Vec<Run> {
    let base = root();
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut runs: Vec<Run> = Vec::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        match read_manifest(&entry.path()) {
            Some(run) => runs.push(run),
            None => {
                let at = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                runs.push(Run {
                    id,
                    at,
                    label: "Unknown run".into(),
                    bytes: 0,
                    entries: Vec::new(),
                });
            }
        }
    }
    runs.sort_by(|a, b| b.at.cmp(&a.at));
    runs
}

/// Total bytes and file count currently held.
pub fn totals() -> (u64, u64) {
    let runs = list();
    let bytes = runs.iter().map(|r| r.bytes).sum();
    let files = runs.iter().map(|r| r.entries.len() as u64).sum();
    (bytes, files)
}

/* ----------------------------------------------------------------- actions */

/// Puts one quarantined file back where it came from.
pub fn restore(run_id: &str, stored: &str) -> Result<PathBuf, String> {
    let dir = run_dir(run_id).ok_or("That quarantine run no longer exists.")?;
    let mut run = read_manifest(&dir).ok_or("That quarantine run has no manifest.")?;
    let entry = run
        .entries
        .iter()
        .find(|e| e.stored == stored)
        .cloned()
        .ok_or("That file is no longer in quarantine.")?;

    if entry.stored.contains("..") {
        return Err("Refusing a suspicious quarantine entry.".into());
    }
    let source = dir.join(&entry.stored);
    if !source.is_file() {
        return Err("The quarantined file is missing.".into());
    }

    let original = PathBuf::from(&entry.original);
    let parent = original
        .parent()
        .ok_or("That entry has no original folder.")?;
    if !crate::core::scanner::is_safe_container(parent, &[]) {
        return Err(
            "G-OpenCleaner will not restore into a protected location, even from quarantine.".into(),
        );
    }
    if original.exists() {
        return Err("A file already exists at that path.".into());
    }

    fs::create_dir_all(parent).map_err(|e| format!("could not recreate the folder: {}", e))?;
    move_file(&source, &original).map_err(|e| format!("could not restore the file: {}", e))?;

    run.entries.retain(|e| e.stored != stored);
    run.bytes = run.entries.iter().map(|e| e.size).sum();
    if run.entries.is_empty() {
        let _ = fs::remove_dir_all(&dir);
    } else {
        let _ = write_manifest(&dir, &run);
    }
    Ok(original)
}

/// Deletes one run for good. Returns the bytes released.
pub fn purge_run(run_id: &str) -> Result<u64, String> {
    let dir = run_dir(run_id).ok_or("That quarantine run no longer exists.")?;
    let bytes = read_manifest(&dir).map(|r| r.bytes).unwrap_or(0);
    fs::remove_dir_all(&dir).map_err(|e| format!("could not purge: {}", e))?;
    Ok(bytes)
}

/// Deletes every run older than `days`. Returns the bytes released.
pub fn purge_expired(days: u64) -> u64 {
    let cutoff = now_secs().saturating_sub(days * 24 * 3600);
    let mut freed = 0;
    for run in list() {
        if run.at > cutoff && run.at != 0 {
            continue;
        }
        if let Ok(bytes) = purge_run(&run.id) {
            freed += bytes;
        }
    }
    freed
}

/// Deletes everything held. Returns the bytes released.
pub fn purge_all() -> u64 {
    let mut freed = 0;
    for run in list() {
        if let Ok(bytes) = purge_run(&run.id) {
            freed += bytes;
        }
    }
    freed
}
