//! Settings and statistics, persisted as JSON under %LOCALAPPDATA%\OpenClean.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())
    });
    PathBuf::from(base).join("OpenClean")
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub confirm_deletes: bool,
    pub secure_delete: bool,
    pub exclusions: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "dark".into(),
            confirm_deletes: true,
            secure_delete: false,
            exclusions: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct HistoryEntry {
    pub at: u64,
    pub bytes: u64,
    pub files: u64,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Stats {
    pub lifetime_bytes: u64,
    pub lifetime_files: u64,
    pub runs: u64,
    pub history: Vec<HistoryEntry>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
struct Doc {
    settings: Settings,
    stats: Stats,
}

pub struct Store {
    path: PathBuf,
    pub settings: Settings,
    pub stats: Stats,
}

impl Store {
    pub fn load() -> Self {
        let dir = data_dir();
        let path = dir.join("store.json");
        let doc: Doc = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        Store {
            path,
            settings: doc.settings,
            stats: doc.stats,
        }
    }

    pub fn save(&self) {
        let doc = Doc {
            settings: self.settings.clone(),
            stats: self.stats.clone(),
        };
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&doc) {
            let tmp = self.path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.path);
            }
        }
    }

    pub fn record_clean(&mut self, bytes: u64, files: u64, label: &str) {
        self.stats.lifetime_bytes += bytes;
        self.stats.lifetime_files += files;
        self.stats.runs += 1;
        self.stats.history.insert(
            0,
            HistoryEntry {
                at: now_secs(),
                bytes,
                files,
                label: label.to_string(),
            },
        );
        self.stats.history.truncate(60);
        self.save();
    }

    pub fn reset_stats(&mut self) {
        self.stats = Stats::default();
        self.save();
    }
}
