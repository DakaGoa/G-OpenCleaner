//! Storage tools: disk analysis, duplicate detection, cloud folder inspection.

use crate::core::model::*;
use crate::core::scanner::{now_millis, size_of};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 400_000;

fn walk_collect(root: &Path, deadline: Instant, max_depth: Option<usize>) -> (Vec<(PathBuf, u64, u64)>, Vec<PathBuf>, bool) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut truncated = false;
    let mut stack = vec![root.to_path_buf()];
    let base_depth = root.components().count();
    let mut seen = 0usize;

    while let Some(dir) = stack.pop() {
        if Instant::now() > deadline || seen > MAX_ENTRIES {
            truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let full = entry.path();
            if ft.is_dir() {
                if let Some(limit) = max_depth {
                    if full.components().count() - base_depth > limit {
                        continue;
                    }
                }
                dirs.push(full.clone());
                stack.push(full);
            } else if ft.is_file() {
                let Ok(meta) = entry.metadata() else { continue };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                files.push((full, meta.len(), mtime));
            }
        }
    }
    (files, dirs, truncated)
}

/* ------------------------------------------------------------- disk usage */

pub fn analyze_disk(root: &Path, depth: usize) -> DiskReport {
    let depth = depth.clamp(1, 6);
    let deadline = Instant::now() + Duration::from_secs(30);
    let root_string = root.to_string_lossy().to_string();
    let base_parts = root_string.split('\\').filter(|p| !p.is_empty()).count().max(1);
    let take = base_parts + depth - 1;

    let (files, _, truncated) = walk_collect(root, deadline, None);
    let mut buckets: HashMap<String, Bucket> = HashMap::new();
    let mut large = Vec::new();
    let mut total = 0u64;

    for (path, size, mtime) in &files {
        total += size;
        let s = path.to_string_lossy().to_string();
        let parts: Vec<&str> = s.split('\\').filter(|p| !p.is_empty()).collect();
        let key = parts[..parts.len().min(take)].join("\\");
        let entry = buckets.entry(key.clone()).or_insert(Bucket { path: key, size: 0, files: 0 });
        entry.size += size;
        entry.files += 1;

        if *size >= 50 * 1024 * 1024 {
            large.push(FileItem { path: s.clone(), size: *size, mtime: *mtime });
        }
    }

    let mut children: Vec<Bucket> = buckets.into_values().collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));
    children.truncate(60);
    large.sort_by(|a, b| b.size.cmp(&a.size));
    large.truncate(40);

    DiskReport {
        root: root_string,
        total,
        file_count: files.len() as u64,
        children,
        large_files: large,
        truncated,
    }
}

/* ------------------------------------------------------------- duplicates */

fn fnv_pair(path: &Path, limit: Option<u64>) -> Option<(u64, u64)> {
    let mut file = File::open(path).ok()?;
    let mut buf = vec![0u8; 65_536];
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut read_total = 0u64;

    loop {
        let want = match limit {
            Some(lim) => {
                if read_total >= lim {
                    break;
                }
                (lim - read_total).min(buf.len() as u64) as usize
            }
            None => buf.len(),
        };
        let n = file.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        for b in &buf[..n] {
            h1 ^= *b as u64;
            h1 = h1.wrapping_mul(0x1000_0000_01b3);
            h2 = h2.rotate_left(5) ^ (*b as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
        read_total += n as u64;
    }
    Some((h1, h2))
}

pub fn find_duplicates(roots: &[String], min_size: u64) -> DupReport {
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut report = DupReport::default();
    let mut all: Vec<(PathBuf, u64, u64)> = Vec::new();

    for root in roots {
        if Instant::now() > deadline {
            report.truncated = true;
            break;
        }
        let (files, _, truncated) = walk_collect(Path::new(root), deadline, Some(14));
        if truncated {
            report.truncated = true;
        }
        report.scanned += files.len() as u64;
        all.extend(files.into_iter().filter(|f| f.1 >= min_size));
    }

    let mut by_size: HashMap<u64, Vec<(PathBuf, u64, u64)>> = HashMap::new();
    for file in all {
        by_size.entry(file.1).or_default().push(file);
    }

    let mut by_head: HashMap<(u64, u64), (u64, Vec<(PathBuf, u64, u64)>)> = HashMap::new();
    for (size, files) in by_size.into_iter().filter(|(_, f)| f.len() > 1) {
        for file in files {
            if Instant::now() > deadline {
                report.truncated = true;
                break;
            }
            if let Some(head) = fnv_pair(&file.0, Some(65_536)) {
                by_head.entry(head).or_insert((size, Vec::new())).1.push(file);
            }
        }
    }

    let mut groups = Vec::new();
    for (_, (size, candidates)) in by_head.into_iter().filter(|(_, (_, f))| f.len() > 1) {
        if Instant::now() > deadline {
            report.truncated = true;
            break;
        }
        let mut by_full: HashMap<(u64, u64), Vec<(PathBuf, u64, u64)>> = HashMap::new();
        for file in candidates {
            if let Some(full) = fnv_pair(&file.0, None) {
                by_full.entry(full).or_default().push(file);
            }
        }
        for (_, mut files) in by_full.into_iter().filter(|(_, f)| f.len() > 1) {
            files.sort_by_key(|f| f.2);
            let wasted = size * (files.len() as u64 - 1);
            groups.push(DupGroup {
                size,
                wasted,
                files: files
                    .into_iter()
                    .map(|(path, size, mtime)| FileItem {
                        path: path.to_string_lossy().to_string(),
                        size,
                        mtime,
                    })
                    .collect(),
            });
        }
    }

    groups.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    report.wasted = groups.iter().map(|g| g.wasted).sum();
    report.groups = groups;
    report
}

/* ------------------------------------------------------------------ cloud */

struct Candidate {
    id: &'static str,
    name: &'static str,
    path: String,
}

fn cloud_candidates() -> Vec<Candidate> {
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let mut out = Vec::new();

    let mut push = |id: &'static str, name: &'static str, path: String| {
        if path.trim().is_empty() || !Path::new(&path).is_dir() {
            return;
        }
        out.push(Candidate { id, name, path });
    };

    push("onedrive", "Microsoft OneDrive", std::env::var("OneDrive").unwrap_or_default());
    push(
        "onedrive-personal",
        "OneDrive (personal)",
        std::env::var("OneDriveConsumer").unwrap_or_default(),
    );
    push(
        "onedrive-work",
        "OneDrive (work or school)",
        std::env::var("OneDriveCommercial").unwrap_or_default(),
    );
    if !home.is_empty() {
        for (id, name, folder) in [
            ("dropbox", "Dropbox", "Dropbox"),
            ("gdrive", "Google Drive", "Google Drive"),
            ("gdrive-my", "Google Drive (My Drive)", "My Drive"),
            ("icloud", "iCloud Drive", "iCloudDrive"),
            ("pcloud", "pCloud Drive", "pCloud Drive"),
            ("nextcloud", "Nextcloud", "Nextcloud"),
        ] {
            push(id, name, format!(r"{}\{}", home, folder));
        }
    }

    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.path.to_ascii_lowercase()));
    out
}

pub fn cloud_drives() -> Vec<CloudDrive> {
    cloud_candidates()
        .into_iter()
        .map(|c| {
            let (size, files) = size_of(Path::new(&c.path));
            let folders = fs::read_dir(&c.path)
                .map(|d| d.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).count())
                .unwrap_or(0);
            let _ = folders;
            CloudDrive {
                id: c.id.to_string(),
                name: c.name.to_string(),
                path: c.path.clone(),
                size,
                files,
                partial: false,
            }
        })
        .collect()
}

pub fn cloud_clutter(root: &Path) -> ClutterReport {
    let deadline = Instant::now() + Duration::from_secs(20);
    let (files, dirs, truncated) = walk_collect(root, deadline, Some(10));
    let now = now_millis();
    let year_ms = 365u64 * 24 * 3600 * 1000;

    let mut old = Vec::new();
    let mut large = Vec::new();
    let mut total = 0u64;
    for (path, size, mtime) in files {
        total += size;
        let item = FileItem {
            path: path.to_string_lossy().to_string(),
            size,
            mtime,
        };
        if mtime > 0 && now.saturating_sub(mtime) > year_ms {
            old.push(item.clone());
        }
        if size > 100 * 1024 * 1024 {
            large.push(item);
        }
    }

    let mut empty = Vec::new();
    for dir in dirs {
        if fs::read_dir(&dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
            empty.push(FileItem {
                path: dir.to_string_lossy().to_string(),
                size: 0,
                mtime: 0,
            });
        }
    }

    old.sort_by(|a, b| b.size.cmp(&a.size));
    large.sort_by(|a, b| b.size.cmp(&a.size));

    ClutterReport {
        root: root.to_string_lossy().to_string(),
        total,
        truncated,
        categories: vec![
            ClutterCategory {
                id: "old",
                name: "Files untouched for over a year",
                size: old.iter().map(|f| f.size).sum(),
                items: old.into_iter().take(200).collect(),
            },
            ClutterCategory {
                id: "large",
                name: "Large files over 100 MB",
                size: large.iter().map(|f| f.size).sum(),
                items: large.into_iter().take(200).collect(),
            },
            ClutterCategory {
                id: "empty",
                name: "Empty folders",
                size: 0,
                items: empty.into_iter().take(300).collect(),
            },
        ],
    }
}
