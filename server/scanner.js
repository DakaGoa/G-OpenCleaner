'use strict';

/**
 * Scanner / cleaner.
 *
 * Safety model: a deletion is only allowed when the resolved path sits strictly
 * inside one of the disposable-cache roots below. Rules cannot opt out of that
 * check, and Windows reparse points (junctions, symlinks) are never followed.
 */

const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const crypto = require('crypto');

const MAX_ENTRIES = 250000;
const MAX_MS_PER_RULE = 25000;

function envMap() {
  const e = process.env;
  return {
    TEMP: e.TEMP || os.tmpdir(),
    TMP: e.TMP || os.tmpdir(),
    LOCALAPPDATA: e.LOCALAPPDATA || '',
    APPDATA: e.APPDATA || '',
    USERPROFILE: e.USERPROFILE || os.homedir(),
    PROGRAMDATA: e.ProgramData || 'C:\\ProgramData',
    ALLUSERSPROFILE: e.ALLUSERSPROFILE || 'C:\\ProgramData',
    PROGRAMFILES: e.ProgramFiles || 'C:\\Program Files',
    'PROGRAMFILES(X86)': e['ProgramFiles(x86)'] || 'C:\\Program Files (x86)',
    WINDIR: e.windir || e.SystemRoot || 'C:\\Windows',
    SYSTEMROOT: e.SystemRoot || 'C:\\Windows',
    SYSTEMDRIVE: e.SystemDrive || 'C:',
    PUBLIC: e.PUBLIC || 'C:\\Users\\Public',
    HOMEDRIVE: e.HOMEDRIVE || 'C:',
    HOMEPATH: e.HOMEPATH || '\\Users',
    ONEDRIVE: e.OneDrive || '',
  };
}

function expandVars(input) {
  if (!input) return '';
  const map = envMap();
  let out = String(input);
  for (let pass = 0; pass < 3; pass++) {
    out = out.replace(/%([A-Za-z0-9_() ]+)%/g, (whole, name) => {
      const key = name.toUpperCase();
      const value = map[key];
      return value ? value : whole;
    });
  }
  if (out.includes('%')) return null; // unresolved variable: skip rather than guess
  return out.replace(/\//g, '\\');
}

/** Roots inside which deletion is always allowed. */
function allowedRoots() {
  const e = process.env;
  const roots = [
    e.TEMP,
    e.TMP,
    os.tmpdir(),
    e.LOCALAPPDATA,
    e.APPDATA,
    e.ProgramData,
    e.ProgramData ? path.join(e.ProgramData, 'Microsoft', 'Windows') : null,
  ].filter(Boolean);

  // Note: %WINDIR% itself is deliberately absent. Only named cache folders
  // inside it are deletable, so a rule can never reach System32 or System.
  const windir = e.windir || e.SystemRoot || 'C:\\Windows';
  roots.push(
    path.join(windir, 'Temp'),
    path.join(windir, 'Logs'),
    path.join(windir, 'Prefetch'),
    path.join(windir, 'Panther'),
    path.join(windir, 'Minidump'),
    path.join(windir, 'LiveKernelReports'),
    path.join(windir, 'SoftwareDistribution', 'Download'),
    path.join(windir, 'SoftwareDistribution', 'DeliveryOptimization'),
    path.join(windir, 'ServiceProfiles', 'LocalService', 'AppData', 'Local')
  );

  return roots
    .filter(Boolean)
    .map((r) => path.resolve(r).replace(/[\\/]+$/, '').toLowerCase())
    .filter((r, i, arr) => arr.indexOf(r) === i);
}

/** Honours the user's exclusion list (exact path or anything beneath it). */
function isExcluded(target, exclusions) {
  if (!exclusions || !exclusions.length) return false;
  const t = path.resolve(target).toLowerCase();
  return exclusions.some((raw) => {
    if (!raw) return false;
    const e = path.resolve(raw).toLowerCase().replace(/[\\/]+$/, '');
    return t === e || t.startsWith(e + '\\');
  });
}

function isInside(child, parent) {
  const c = path.resolve(child).toLowerCase();
  const p = path.resolve(parent).toLowerCase();
  if (c === p) return false; // never delete the root itself
  return c.startsWith(p.endsWith('\\') ? p : p + '\\');
}

/** Individual files that are safe to remove even though their folder is not. */
function allowedFiles() {
  const windir = process.env.windir || process.env.SystemRoot || 'C:\\Windows';
  return [path.join(windir, 'MEMORY.DMP')].map((p) => path.resolve(p).toLowerCase());
}

/** Verifies a path is safe to delete (strong roots first, then extra roots). */
function isSafeToDelete(target, extraRoots = []) {
  if (!target) return false;
  const resolved = path.resolve(target);
  if (resolved.length < 8) return false;
  if (/^[a-z]:\\?$/i.test(resolved)) return false;

  const lower = resolved.toLowerCase();
  if (allowedFiles().includes(lower)) return true;

  for (const root of allowedRoots()) {
    if (isInside(resolved, root)) return true;
  }
  for (const root of extraRoots) {
    if (root && isInside(resolved, root)) return true;
  }
  return false;
}

function isReparsePoint(stat) {
  return !!(stat && stat.isSymbolicLink && (stat.isSymbolicLink() || (stat.mode & 0o40000 && false)));
}

/**
 * A folder may be *entered* when it equals or sits inside an allowed root.
 * Its children still have to pass the stricter isSafeToDelete() check before
 * anything is removed, so the root folder itself is never deleted.
 */
function isSafeContainer(target, extraRoots = []) {
  if (!target) return false;
  const resolved = path.resolve(target);
  const lower = resolved.toLowerCase();
  if (/^[a-z]:\\?$/i.test(lower)) return false;

  for (const root of allowedRoots()) {
    if (lower === root || isInside(resolved, root)) return true;
  }
  for (const root of extraRoots) {
    if (!root) continue;
    const r = path.resolve(root).toLowerCase().replace(/[\\/]+$/, '');
    if (lower === r || lower.startsWith(r + '\\')) return true;
  }
  return false;
}

async function lstatSafe(p) {
  try {
    return await fsp.lstat(p);
  } catch {
    return null;
  }
}

/** Expands `*` path segments (browser profiles, etc). Capped to keep scans fast. */
function expandGlob(input, cap = 64) {
  if (!input) return [];
  const parts = input.split('\\').filter((p, i) => p !== '' || i === 0);
  let current = [parts[0] + '\\'];
  for (let i = 1; i < parts.length; i++) {
    const seg = parts[i];
    const next = [];
    for (const base of current) {
      if (!seg.includes('*')) {
        next.push(path.join(base, seg));
        continue;
      }
      let entries = [];
      try {
        entries = fs.readdirSync(base, { withFileTypes: true });
      } catch {
        continue;
      }
      const re = new RegExp('^' + seg.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*') + '$', 'i');
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        if (re.test(entry.name)) next.push(path.join(base, entry.name));
        if (next.length >= cap) break;
      }
    }
    current = next;
    if (!current.length) return [];
  }
  return current;
}

function matchPattern(name, pattern) {
  if (!pattern || pattern === '*') return true;
  const re = new RegExp('^' + pattern.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*') + '$', 'i');
  return re.test(name);
}

/**
 * Walks a directory collecting files.
 * Returns { files: [{path,size,mtime}], dirs: [path] }
 */
async function walk(dir, opts = {}) {
  const out = { files: [], dirs: [] };
  const deadline = Date.now() + (opts.maxMs || MAX_MS_PER_RULE);
  const stack = [dir];
  let seen = 0;

  while (stack.length) {
    if (Date.now() > deadline || seen > MAX_ENTRIES) {
      out.truncated = true;
      break;
    }
    const current = stack.pop();
    let entries;
    try {
      entries = await fsp.readdir(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (++seen > MAX_ENTRIES) { out.truncated = true; break; }
      const full = path.join(current, entry.name);
      if (entry.isSymbolicLink()) continue; // never traverse or delete through links
      if (entry.isDirectory()) {
        if (opts.maxDepth !== undefined && full.split('\\').length - dir.split('\\').length > opts.maxDepth) continue;
        if (!opts.skipDirs) stack.push(full);
        out.dirs.push(full);
      } else if (entry.isFile()) {
        if (opts.pattern && !matchPattern(entry.name, opts.pattern)) continue;
        const st = await lstatSafe(full);
        if (!st) continue;
        if (opts.olderThanHours) {
          const ageHours = (Date.now() - st.mtimeMs) / 36e5;
          if (ageHours < opts.olderThanHours) continue;
        }
        out.files.push({ path: full, size: st.size, mtime: st.mtimeMs });
      }
    }
  }
  return out;
}

/** Size of a file or folder (never follows reparse points). */
async function sizeOf(target, opts = {}) {
  const st = await lstatSafe(target);
  if (!st) return { size: 0, files: 0, dirs: 0 };
  if (!st.isDirectory()) return { size: st.size, files: 1, dirs: 0 };
  const walked = await walk(target, { ...opts, maxMs: opts.maxMs || 20000 });
  let size = 0;
  for (const f of walked.files) size += f.size;
  return { size, files: walked.files.length, dirs: walked.dirs.length, truncated: walked.truncated };
}

/* ------------------------------------------------------------------ *
 * Scan
 * ------------------------------------------------------------------ */

async function targetsForRule(rule) {
  const out = [];
  for (const target of rule.targets || []) {
    if (target.mode === 'action') {
      out.push({ ...target, resolved: ['<action>'] });
      continue;
    }
    const expanded = expandVars(target.path);
    if (!expanded) continue;
    if (expanded.includes('*')) {
      const matches = expandGlob(expanded);
      if (matches.length) out.push({ ...target, resolved: matches });
    } else {
      const st = await lstatSafe(expanded);
      if (st) out.push({ ...target, resolved: [expanded] });
    }
  }
  return out;
}

/** Does this rule apply to the current machine at all? */
async function ruleApplies(rule) {
  const targets = await targetsForRule(rule);
  return targets.length > 0;
}

const ACTIONS = {
  clipboard: { label: 'Clear clipboard', run: clearClipboard },
  dns: { label: 'Flush DNS cache', run: flushDns },
  recyclebin: { label: 'Empty Recycle Bin', run: () => require('./win').emptyRecycleBin() },
};

async function clearClipboard() {
  return new Promise((resolve) => {
    const ps = require('./win').ps;
    ps('Set-Clipboard -Value $null; $true | ConvertTo-Json', 15000).then((r) =>
      resolve({ ok: r.ok, error: r.ok ? null : 'Could not clear the clipboard.' })
    );
  });
}

async function flushDns() {
  const { run } = require('./win');
  const res = await run('ipconfig.exe', ['/flushdns'], { timeout: 20000 });
  return { ok: res.ok, error: res.ok ? null : 'ipconfig /flushdns failed.' };
}

/**
 * Collects everything a rule would remove.
 * @returns {{rule, size, count, samples, actions, error, truncated}}
 */
async function scanRule(rule, opts = {}) {
  const started = Date.now();
  const targets = await targetsForRule(rule);
  const result = {
    id: rule.id,
    name: rule.name,
    group: rule.group,
    desc: rule.desc || '',
    browser: rule.browser || null,
    default: !!rule.default,
    warning: !!rule.warning,
    size: 0,
    count: 0,
    samples: [],
    actions: [],
    applicable: targets.length > 0,
    error: null,
    truncated: false,
  };

  for (const target of targets) {
    if (Date.now() - started > MAX_MS_PER_RULE * 2) { result.truncated = true; break; }

    if (target.mode === 'action') {
      result.actions.push(target.action);
      continue;
    }

    for (const resolved of target.resolved) {
      const safe = target.mode === 'file'
        ? isSafeToDelete(resolved, opts.extraRoots || [])
        : isSafeContainer(resolved, opts.extraRoots || []);
      if (!safe) {
        result.error = result.error || 'Skipped a location outside the allowed cache roots.';
        continue;
      }
      const stat = await lstatSafe(resolved);
      if (!stat) continue;

      if (target.mode === 'file') {
        if (stat.isDirectory()) continue;
        if (isExcluded(resolved, opts.exclusions)) continue;
        result.size += stat.size;
        result.count += 1;
        if (result.samples.length < 24) result.samples.push({ path: resolved, size: stat.size });
        continue;
      }

      if (target.mode === 'contents') {
        if (!stat.isDirectory()) continue;
        let entries = [];
        try {
          entries = await fsp.readdir(resolved, { withFileTypes: true });
        } catch {
          continue;
        }
        for (const entry of entries) {
          const full = path.join(resolved, entry.name);
          if (entry.isSymbolicLink()) continue;
          if (isExcluded(full, opts.exclusions)) continue;
          const info = await sizeOf(full, { maxMs: 6000 });
          result.size += info.size;
          result.count += info.files || (entry.isDirectory() ? 0 : 1);
          if (result.samples.length < 24) result.samples.push({ path: full, size: info.size });
          if (info.truncated) result.truncated = true;
        }
        continue;
      }

      if (target.mode === 'recurse') {
        if (!stat.isDirectory()) continue;
        const walked = await walk(resolved, {
          pattern: target.pattern,
          olderThanHours: target.olderThanHours,
        });
        for (const file of walked.files) {
          if (isExcluded(file.path, opts.exclusions)) continue;
          result.size += file.size;
          result.count += 1;
          if (result.samples.length < 24) result.samples.push({ path: file.path, size: file.size });
        }
        if (walked.truncated) result.truncated = true;
      }
    }
  }

  return result;
}

async function scanRules(rulesToScan, opts = {}) {
  const results = [];
  for (const rule of rulesToScan) {
    try {
      results.push(await scanRule(rule, opts));
    } catch (err) {
      results.push({
        id: rule.id,
        name: rule.name,
        group: rule.group,
        desc: rule.desc || '',
        browser: rule.browser || null,
        size: 0,
        count: 0,
        samples: [],
        actions: [],
        applicable: true,
        error: err.message,
        truncated: false,
      });
    }
  }
  return results;
}

/* ------------------------------------------------------------------ *
 * Clean
 * ------------------------------------------------------------------ */

async function secureWipe(file, stat) {
  try {
    const handle = await fsp.open(file, 'r+');
    try {
      const chunk = crypto.randomBytes(65536);
      let remaining = Math.min(stat.size, 8 * 1024 * 1024);
      let pos = 0;
      while (remaining > 0) {
        const len = Math.min(chunk.length, remaining);
        await handle.write(chunk, 0, len, pos);
        pos += len;
        remaining -= len;
      }
      await handle.sync();
    } finally {
      await handle.close();
    }
  } catch {
    /* best effort: an unwritable file is simply removed normally */
  }
}

async function removePath(target, opts = {}) {
  const stat = await lstatSafe(target);
  if (!stat) return { removed: 0, bytes: 0, skipped: 0 };

  if (!isSafeToDelete(target, opts.extraRoots || [])) {
    return { removed: 0, bytes: 0, skipped: 1, reason: 'outside allowed roots' };
  }

  if (isExcluded(target, opts.exclusions)) {
    return { removed: 0, bytes: 0, skipped: 1, reason: 'excluded' };
  }

  if (stat.isDirectory()) {
    let removed = 0;
    let bytes = 0;
    let skipped = 0;
    let entries = [];
    try {
      entries = await fsp.readdir(target, { withFileTypes: true });
    } catch {
      return { removed: 0, bytes: 0, skipped: 1 };
    }
    for (const entry of entries) {
      const full = path.join(target, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (isExcluded(full, opts.exclusions)) { skipped += 1; continue; }
      if (entry.isDirectory()) {
        const inner = await removePath(full, opts);
        removed += inner.removed;
        bytes += inner.bytes;
        skipped += inner.skipped;
        try { await fsp.rmdir(full); } catch { /* not empty or in use */ }
      } else {
        const fileStat = await lstatSafe(full);
        if (!fileStat) continue;
        try {
          if (opts.secure) await secureWipe(full, fileStat);
          await fsp.unlink(full);
          removed += 1;
          bytes += fileStat.size;
        } catch {
          skipped += 1;
        }
      }
    }
    // Remove the now-empty directory itself. When the user asked to empty a
    // cache folder, that folder is the *parent* passed to this function, so it
    // is unaffected.
    try { await fsp.rmdir(target); } catch { /* in use or still has exclusions */ }
    return { removed, bytes, skipped };
  }

  try {
    if (opts.secure) await secureWipe(target, stat);
    await fsp.unlink(target);
    return { removed: 1, bytes: stat.size, skipped: 0 };
  } catch {
    return { removed: 0, bytes: 0, skipped: 1 };
  }
}

async function cleanRule(rule, opts = {}) {
  const targets = await targetsForRule(rule);
  const outcome = {
    id: rule.id,
    name: rule.name,
    group: rule.group,
    bytes: 0,
    removed: 0,
    skipped: 0,
    actions: [],
    errors: [],
  };

  for (const target of targets) {
    if (target.mode === 'action') {
      const action = ACTIONS[target.action];
      if (!action) continue;
      try {
        const res = await action.run();
        outcome.actions.push({ action: target.action, label: action.label, ok: !!(res && res.ok), error: res && res.error });
        if (target.action === 'recyclebin' && res && res.bytes) outcome.bytes += res.bytes;
      } catch (err) {
        outcome.actions.push({ action: target.action, label: action.label, ok: false, error: err.message });
      }
      continue;
    }

    for (const resolved of target.resolved) {
      const safe = target.mode === 'file'
        ? isSafeToDelete(resolved, opts.extraRoots || [])
        : isSafeContainer(resolved, opts.extraRoots || []);
      if (!safe) {
        outcome.skipped += 1;
        continue;
      }

      const stat = await lstatSafe(resolved);
      if (!stat) continue;

      if (target.mode === 'file') {
        const res = await removePath(resolved, opts);
        outcome.bytes += res.bytes;
        outcome.removed += res.removed;
        outcome.skipped += res.skipped;
        continue;
      }

      if (target.mode === 'contents') {
        if (!stat.isDirectory()) continue;
        let entries = [];
        try {
          entries = await fsp.readdir(resolved, { withFileTypes: true });
        } catch {
          outcome.skipped += 1;
          continue;
        }
        for (const entry of entries) {
          const full = path.join(resolved, entry.name);
          if (entry.isSymbolicLink()) continue;
          const res = await removePath(full, opts);
          outcome.bytes += res.bytes;
          outcome.removed += res.removed;
          outcome.skipped += res.skipped;
        }
        continue;
      }

      if (target.mode === 'recurse') {
        if (!stat.isDirectory()) continue;
        const walked = await walk(resolved, {
          pattern: target.pattern,
          olderThanHours: target.olderThanHours,
        });
        for (const file of walked.files) {
          const res = await removePath(file.path, { ...opts, mode: 'file' });
          outcome.bytes += res.bytes;
          outcome.removed += res.removed;
          outcome.skipped += res.skipped;
        }
      }
    }
  }

  return outcome;
}

module.exports = {
  envMap,
  expandVars,
  expandGlob,
  allowedRoots,
  allowedFiles,
  isSafeToDelete,
  isSafeContainer,
  isInside,
  walk,
  sizeOf,
  scanRule,
  scanRules,
  cleanRule,
  removePath,
  ruleApplies,
  targetsForRule,
  isExcluded,
  ACTIONS,
};
