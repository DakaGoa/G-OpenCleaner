'use strict';

/** Settings + statistics persistence (single JSON file, atomic writes). */

const fs = require('fs');
const path = require('path');

const DATA_DIR = path.join(__dirname, '..', 'data');
const FILE = path.join(DATA_DIR, 'store.json');

const DEFAULTS = {
  settings: {
    theme: 'dark',
    confirmDeletes: true,
    secureDelete: false,
    exclusions: [],
  },
  stats: {
    lifetimeBytes: 0,
    lifetimeFiles: 0,
    runs: 0,
    history: [],
  },
};

let cache = null;

function load() {
  if (cache) return cache;
  try {
    const raw = fs.readFileSync(FILE, 'utf8');
    const parsed = JSON.parse(raw);
    cache = {
      settings: { ...DEFAULTS.settings, ...(parsed.settings || {}) },
      stats: { ...DEFAULTS.stats, ...(parsed.stats || {}) },
    };
  } catch {
    cache = JSON.parse(JSON.stringify(DEFAULTS));
  }
  return cache;
}

function save() {
  try {
    fs.mkdirSync(DATA_DIR, { recursive: true });
    const tmp = `${FILE}.${process.pid}.tmp`;
    fs.writeFileSync(tmp, JSON.stringify(cache, null, 2), 'utf8');
    fs.renameSync(tmp, FILE);
  } catch {
    /* the app keeps working in memory if the disk is read-only */
  }
}

function settings() {
  return load().settings;
}

function updateSettings(patch) {
  const state = load();
  state.settings = { ...state.settings, ...patch };
  save();
  return state.settings;
}

function stats() {
  return load().stats;
}

function recordClean({ bytes = 0, files = 0, label = 'Clean', details = [] }) {
  const state = load();
  state.stats.lifetimeBytes += bytes;
  state.stats.lifetimeFiles += files;
  state.stats.runs += 1;
  state.stats.history.unshift({
    at: new Date().toISOString(),
    bytes,
    files,
    label,
    details: details.slice(0, 12),
  });
  state.stats.history = state.stats.history.slice(0, 60);
  save();
  return state.stats;
}

function resetStats() {
  const state = load();
  state.stats = JSON.parse(JSON.stringify(DEFAULTS.stats));
  save();
  return state.stats;
}

module.exports = { settings, updateSettings, stats, recordClean, resetStats, FILE };
