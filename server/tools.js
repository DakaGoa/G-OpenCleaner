'use strict';

/** Storage tools: disk analyzer, duplicate finder, cloud drive inspection. */

const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const scanner = require('./scanner');

const SIZE_UNITS = { B: 1, KB: 1024, MB: 1024 ** 2, GB: 1024 ** 3 };

async function readdirSafe(dir) {
  try {
    return await fsp.readdir(dir, { withFileTypes: true });
  } catch {
    return [];
  }
}

/**
 * Single-pass disk analysis.
 * Buckets every file by its Nth path segment so the UI can show "C:\Users  42 GB".
 */
async function analyzeDisk(root, opts = {}) {
  const depth = Math.max(1, Math.min(6, opts.depth || 2));
  const deadline = Date.now() + (opts.maxMs || 30000);
  const rootResolved = path.resolve(root);
  const baseParts = rootResolved.split('\\').filter(Boolean);

  const buckets = new Map();
  const largeFiles = [];
  let total = 0;
  let fileCount = 0;
  let truncated = false;
  let denied = 0;

  const stack = [rootResolved];
  while (stack.length) {
    if (Date.now() > deadline) { truncated = true; break; }
    const dir = stack.pop();
    const entries = await readdirSafe(dir);
    if (!entries.length) {
      try { await fsp.access(dir, fs.constants.R_OK); } catch { denied += 1; }
      continue;
    }
    for (const entry of entries) {
      const full = path.join(dir, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) {
        stack.push(full);
        continue;
      }
      if (!entry.isFile()) continue;
      let stat;
      try {
        stat = await fsp.lstat(full);
      } catch {
        continue;
      }
      total += stat.size;
      fileCount += 1;

      const parts = full.split('\\').filter(Boolean);
      const bucketParts = parts.slice(0, baseParts.length + depth - 1);
      const bucketPath = bucketParts.join('\\');
      const current = buckets.get(bucketPath) || { path: bucketPath, size: 0, files: 0 };
      current.size += stat.size;
      current.files += 1;
      buckets.set(bucketPath, current);

      if (stat.size >= 50 * 1024 ** 2) {
        largeFiles.push({ path: full, size: stat.size, mtime: stat.mtimeMs });
      }
    }
  }

  largeFiles.sort((a, b) => b.size - a.size);

  return {
    root: rootResolved,
    total,
    fileCount,
    depth,
    truncated,
    denied,
    children: [...buckets.values()].sort((a, b) => b.size - a.size).slice(0, 60),
    largeFiles: largeFiles.slice(0, 40),
  };
}

/* ------------------------------------------------------------------ *
 * Duplicate finder
 * ------------------------------------------------------------------ */

function hashFile(file, bytes) {
  return new Promise((resolve) => {
    const hash = crypto.createHash('md5');
    const stream = fs.createReadStream(file, bytes ? { start: 0, end: bytes - 1 } : undefined);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('error', () => resolve(null));
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}

async function findDuplicates(roots, opts = {}) {
  const minSize = Math.max(1024, opts.minSize || 64 * 1024);
  const deadline = Date.now() + (opts.maxMs || 45000);
  const files = [];
  let scanned = 0;
  let truncated = false;

  for (const root of roots) {
    if (Date.now() > deadline) { truncated = true; break; }
    const walked = await scanner.walk(path.resolve(root), {
      maxMs: Math.max(3000, deadline - Date.now()),
      maxDepth: opts.maxDepth || 12,
    });
    if (walked.truncated) truncated = true;
    for (const file of walked.files) {
      scanned += 1;
      if (file.size >= minSize) files.push(file);
    }
  }

  // Stage 1: identical size.
  const bySize = new Map();
  for (const file of files) {
    const list = bySize.get(file.size) || [];
    list.push(file);
    bySize.set(file.size, list);
  }

  const candidates = [...bySize.entries()].filter(([, list]) => list.length > 1);

  // Stage 2: identical first 64 KiB.
  const byHead = new Map();
  for (const [size, list] of candidates) {
    for (const file of list) {
      if (Date.now() > deadline) { truncated = true; break; }
      const head = await hashFile(file.path, Math.min(size, 65536));
      if (!head) continue;
      const key = `${size}:${head}`;
      const bucket = byHead.get(key) || { size, files: [] };
      bucket.files.push(file);
      byHead.set(key, bucket);
    }
  }

  // Stage 3: full hash for the survivors.
  const groups = [];
  for (const bucket of byHead.values()) {
    if (bucket.files.length < 2) continue;
    if (Date.now() > deadline) { truncated = true; break; }
    const byFull = new Map();
    for (const file of bucket.files) {
      const full = await hashFile(file.path);
      if (!full) continue;
      const list = byFull.get(full) || [];
      list.push(file);
      byFull.set(full, list);
    }
    for (const [hash, list] of byFull) {
      if (list.length < 2) continue;
      groups.push({
        hash,
        size: list[0].size,
        count: list.length,
        wasted: list[0].size * (list.length - 1),
        files: list.map((f) => ({ path: f.path, size: f.size, mtime: f.mtime })).sort((a, b) => b.mtime - a.mtime),
      });
    }
  }

  groups.sort((a, b) => b.wasted - a.wasted);

  return {
    groups: groups.slice(0, 250),
    groupCount: groups.length,
    wasted: groups.reduce((sum, g) => sum + g.wasted, 0),
    scanned,
    candidates: files.length,
    truncated,
    minSize,
  };
}

/* ------------------------------------------------------------------ *
 * Cloud drives
 * ------------------------------------------------------------------ */

function cloudCandidates() {
  const home = os.homedir();
  const env = process.env;
  const found = [
    { id: 'onedrive', name: 'Microsoft OneDrive', path: env.OneDrive || '' },
    { id: 'onedrive-personal', name: 'OneDrive (personal)', path: env.OneDriveConsumer || '' },
    { id: 'onedrive-work', name: 'OneDrive (work or school)', path: env.OneDriveCommercial || '' },
    { id: 'dropbox', name: 'Dropbox', path: path.join(home, 'Dropbox') },
    { id: 'gdrive', name: 'Google Drive', path: path.join(home, 'Google Drive') },
    { id: 'gdrive-my', name: 'Google Drive (My Drive)', path: path.join(home, 'My Drive') },
    { id: 'icloud', name: 'iCloud Drive', path: path.join(home, 'iCloudDrive') },
    { id: 'pcloud', name: 'pCloud Drive', path: path.join(home, 'pCloud Drive') },
    { id: 'nextcloud', name: 'Nextcloud', path: path.join(home, 'Nextcloud') },
  ];

  const seen = new Set();
  return found.filter((c) => {
    if (!c.path) return false;
    const key = path.resolve(c.path).toLowerCase();
    if (seen.has(key)) return false;
    try {
      if (!fs.statSync(c.path).isDirectory()) return false;
    } catch {
      return false;
    }
    seen.add(key);
    return true;
  });
}

async function cloudDrives() {
  const drives = cloudCandidates();
  const out = [];
  for (const drive of drives) {
    const info = await scanner.sizeOf(drive.path, { maxMs: 9000, maxDepth: 10 });
    const entries = await readdirSafe(drive.path);
    out.push({
      id: drive.id,
      name: drive.name,
      path: drive.path,
      size: info.size,
      files: info.files,
      partial: !!info.truncated,
      folders: entries.filter((e) => e.isDirectory()).length,
    });
  }
  return out;
}

/**
 * Clutter inside a cloud folder: files nobody has opened this year, very large
 * files, and empty folders. Everything is reported, nothing is removed here.
 */
async function cloudClutter(root, opts = {}) {
  const rootResolved = path.resolve(root);
  const walked = await scanner.walk(rootResolved, { maxMs: opts.maxMs || 20000, maxDepth: 10 });
  const yearAgo = Date.now() - 365 * 24 * 3600 * 1000;

  const oldFiles = [];
  const largeFiles = [];
  let totalSize = 0;

  for (const file of walked.files) {
    totalSize += file.size;
    if (file.mtime < yearAgo) oldFiles.push({ path: file.path, size: file.size, mtime: file.mtime });
    if (file.size > 100 * 1024 ** 2) largeFiles.push({ path: file.path, size: file.size, mtime: file.mtime });
  }

  const emptyDirs = [];
  for (const dir of walked.dirs) {
    const entries = await readdirSafe(dir);
    if (!entries.length) emptyDirs.push({ path: dir, size: 0 });
  }

  oldFiles.sort((a, b) => b.size - a.size);
  largeFiles.sort((a, b) => b.size - a.size);

  return {
    root: rootResolved,
    totalSize,
    truncated: !!walked.truncated,
    categories: [
      { id: 'old', name: 'Files untouched for over a year', size: oldFiles.reduce((s, f) => s + f.size, 0), items: oldFiles.slice(0, 200) },
      { id: 'large', name: 'Large files over 100 MB', size: largeFiles.reduce((s, f) => s + f.size, 0), items: largeFiles.slice(0, 200) },
      { id: 'empty', name: 'Empty folders', size: 0, items: emptyDirs.slice(0, 300) },
    ],
  };
}

module.exports = { analyzeDisk, findDuplicates, cloudDrives, cloudClutter, cloudCandidates, SIZE_UNITS };
