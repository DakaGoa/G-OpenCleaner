'use strict';

/**
 * JSON API. Every destructive route requires `confirm: true` in the body, on
 * top of the session token enforced by the HTTP layer.
 */

const path = require('path');
const fs = require('fs');
const win = require('./win');
const scanner = require('./scanner');
const tools = require('./tools');
const store = require('./store');
const { rules, groups } = require('./rules');

let ruleMetaCache = null;
let ruleMetaAt = 0;

function requireConfirm(body) {
  if (!body || body.confirm !== true) {
    const err = new Error('This action needs explicit confirmation.');
    err.code = 'CONFIRM';
    throw err;
  }
}

function getRule(id) {
  const rule = rules.find((r) => r.id === id);
  if (!rule) throw new Error(`Unknown rule: ${id}`);
  return rule;
}

function ruleMeta(rule) {
  return {
    id: rule.id,
    name: rule.name,
    group: rule.group,
    desc: rule.desc || '',
    browser: rule.browser || null,
    browserId: rule.browserId || null,
    default: !!rule.default,
    warning: !!rule.warning,
    // permanent => data cannot be recovered; healthFix => safe for one-click fix
    permanent: !!rule.permanent,
    healthFix: rule.healthFix !== false,
    targets: (rule.targets || []).length,
  };
}

/** Human labels for the system actions a rule can trigger. */
function actionLabels(results) {
  const labels = [];
  for (const result of results) {
    for (const action of result.actions || []) {
      const entry = scanner.ACTIONS[action];
      const label = entry ? entry.label : action;
      if (!labels.includes(label)) labels.push(label);
    }
  }
  return labels;
}

async function ruleMetadata(force) {
  if (!force && ruleMetaCache && Date.now() - ruleMetaAt < 60000) return ruleMetaCache;
  const out = [];
  for (const rule of rules) {
    let applicable = true;
    try {
      applicable = await scanner.ruleApplies(rule);
    } catch {
      applicable = false;
    }
    out.push({ ...ruleMeta(rule), applicable });
  }
  ruleMetaCache = out;
  ruleMetaAt = Date.now();
  return out;
}

function cleanOpts(body) {
  const settings = store.settings();
  return {
    secure: body && body.secure !== undefined ? !!body.secure : !!settings.secureDelete,
    exclusions: settings.exclusions || [],
  };
}

async function scanSelected(ruleIds, opts) {
  const ids = Array.isArray(ruleIds) && ruleIds.length ? ruleIds : rules.map((r) => r.id);
  const selected = ids.map(getRule);
  const results = await scanner.scanRules(selected, opts);
  return results;
}

/* ------------------------------------------------------------------ *
 * Health check
 * ------------------------------------------------------------------ */

function scoreForJunk(bytes) {
  const gb = bytes / 1024 ** 3;
  if (gb > 5) return 40;
  if (gb > 1) return 26;
  if (gb > 0.3) return 14;
  if (gb > 0.05) return 6;
  return 0;
}

async function health() {
  const settings = store.settings();
  const opts = { exclusions: settings.exclusions || [] };

  // Health Check only fixes things that regenerate on their own. Rules that
  // destroy user data (Recycle Bin) or discard user state (clipboard) are
  // excluded here and remain explicit choices in Custom Clean.
  const junkRules = rules.filter((r) => r.default && r.group !== 'Browser cleaner' && r.healthFix !== false);
  const privacyRules = rules.filter((r) => r.browser && r.default && ['cookies', 'history', 'session', 'forms'].includes(r.id.split('.').pop()));

  const [junkResults, privacyResults, startup, info, drivers] = await Promise.all([
    scanner.scanRules(junkRules, opts).catch(() => []),
    scanner.scanRules(privacyRules, opts).catch(() => []),
    win.startupItems().catch(() => []),
    win.systemInfo().catch(() => ({ drives: [], memTotal: 0, memFree: 0 })),
    win.drivers().catch(() => []),
  ]);

  const junkBytes = junkResults.reduce((s, r) => s + r.size, 0);
  const junkFiles = junkResults.reduce((s, r) => s + r.count, 0);
  const privacyItems = privacyResults.reduce((s, r) => s + r.count, 0);
  const enabledStartup = startup.filter((i) => i.enabled).length;

  const systemDrive = (info.drives || []).find((d) => d.letter === 'C:') || (info.drives || [])[0] || null;
  const freePercent = systemDrive && systemDrive.total ? (systemDrive.free / systemDrive.total) * 100 : 0;
  const freeBytes = systemDrive ? systemDrive.free : 0;
  const memUsedPercent = info.memTotal ? ((info.memTotal - info.memFree) / info.memTotal) * 100 : 0;

  const oldDriverCutoff = Date.now() - 3 * 365 * 24 * 3600 * 1000;
  const oldDrivers = (drivers || []).filter((d) => d.date && new Date(d.date).getTime() < oldDriverCutoff);

  const checks = [
    {
      id: 'junk',
      title: 'Junk files',
      detail:
        junkBytes > 0
          ? `${junkFiles.toLocaleString()} files can be removed`
          : 'Nothing to clean up',
      value: junkBytes,
      valueText: formatBytes(junkBytes),
      status: junkBytes > 300 * 1024 ** 2 ? 'bad' : junkBytes > 40 * 1024 ** 2 ? 'warn' : 'good',
      penalty: scoreForJunk(junkBytes),
      fixRuleIds: junkResults.filter((r) => r.count > 0).map((r) => r.id),
      fixActions: actionLabels(junkResults),
    },
    {
      id: 'privacy',
      title: 'Privacy traces',
      detail: privacyItems > 0 ? `${privacyItems} browser traces found` : 'No browsing traces found',
      value: privacyItems,
      valueText: `${privacyItems} items`,
      status: privacyItems > 200 ? 'bad' : privacyItems > 0 ? 'warn' : 'good',
      penalty: privacyItems > 200 ? 15 : privacyItems > 0 ? 7 : 0,
      fixRuleIds: privacyResults.filter((r) => r.count > 0).map((r) => r.id),
      fixActions: actionLabels(privacyResults),
    },
    {
      id: 'startup',
      title: 'Startup impact',
      detail: enabledStartup > 0 ? `${enabledStartup} programs run at sign-in` : 'Clean start-up',
      value: enabledStartup,
      valueText: `${enabledStartup} apps`,
      status: enabledStartup > 10 ? 'bad' : enabledStartup > 5 ? 'warn' : 'good',
      penalty: enabledStartup > 10 ? 15 : enabledStartup > 5 ? 7 : 0,
      link: 'startup',
    },
    {
      id: 'disk',
      title: 'Disk space',
      detail: systemDrive ? `${formatBytes(freeBytes)} free of ${formatBytes(systemDrive.total)}` : 'No fixed drive found',
      value: freePercent,
      valueText: `${freePercent.toFixed(0)}% free`,
      status: freePercent < 10 ? 'bad' : freePercent < 20 ? 'warn' : 'good',
      penalty: freePercent < 10 ? 20 : freePercent < 20 ? 10 : 0,
      link: 'disk',
    },
    {
      id: 'memory',
      title: 'Memory pressure',
      detail: `${formatBytes((info.memTotal || 0) - (info.memFree || 0))} of ${formatBytes(info.memTotal || 0)} in use`,
      value: memUsedPercent,
      valueText: `${memUsedPercent.toFixed(0)}% used`,
      status: memUsedPercent > 88 ? 'bad' : memUsedPercent > 72 ? 'warn' : 'good',
      penalty: memUsedPercent > 88 ? 12 : memUsedPercent > 72 ? 6 : 0,
      link: 'performance',
    },
    {
      id: 'drivers',
      title: 'Driver age',
      detail: oldDrivers.length ? `${oldDrivers.length} drivers are over three years old` : 'Drivers look recent',
      value: oldDrivers.length,
      valueText: `${oldDrivers.length} old`,
      status: oldDrivers.length > 8 ? 'warn' : 'good',
      penalty: oldDrivers.length > 8 ? 6 : 0,
      link: 'drivers',
    },
  ];

  const score = Math.max(0, Math.min(100, 100 - checks.reduce((s, c) => s + c.penalty, 0)));

  // The exact plan the Fix button will execute, so the dialog can describe it
  // truthfully instead of summarising it away.
  const fixResults = [...junkResults, ...privacyResults].filter(
    (r) => r.count > 0 || (r.actions || []).length
  );
  const fixPlan = {
    bytes: fixResults.reduce((sum, r) => sum + r.size, 0),
    count: fixResults.reduce((sum, r) => sum + r.count, 0),
    rules: fixResults.map((r) => ({
      id: r.id,
      name: r.name,
      group: r.group,
      bytes: r.size,
      count: r.count,
    })),
    actions: actionLabels(fixResults),
    // Stated in the UI so nobody assumes Fix also empties the bin.
    excluded: rules
      .filter((r) => r.healthFix === false)
      .map((r) => ({ id: r.id, name: r.name, permanent: !!r.permanent })),
  };

  return {
    score,
    checkedAt: new Date().toISOString(),
    checks,
    fixPlan,
    summary: {
      junkBytes,
      junkFiles,
      privacyItems,
      startupItems: enabledStartup,
      freeBytes,
      diskTotal: systemDrive ? systemDrive.total : 0,
      memUsedPercent,
      oldDrivers: oldDrivers.length,
    },
  };
}

/* ------------------------------------------------------------------ *
 * Route table
 * ------------------------------------------------------------------ */

const handlers = {
  'GET /api/system': async () => {
    const [info, recycle] = await Promise.all([win.systemInfo(), win.recycleBin()]);
    return { ...info, recycleBin: recycle, settings: store.settings(), stats: store.stats() };
  },

  'GET /api/health': async () => health(),

  'POST /api/health/fix': async (body) => {
    requireConfirm(body);
    const result = await runClean(body.ruleIds, cleanOpts(body), body.label || 'Health Check');
    const next = await health();
    return { ...result, health: next };
  },

  'GET /api/rules': async () => {
    const meta = await ruleMetadata();
    return { rules: meta, groups };
  },

  'POST /api/rules/scan': async (body) =>
    ({ results: await scanSelected(body.ruleIds, { exclusions: store.settings().exclusions || [] }) }),

  'POST /api/rules/clean': async (body) => {
    requireConfirm(body);
    return runClean(body.ruleIds, cleanOpts(body), body.label || 'Custom Clean');
  },

  'GET /api/browsers': async () => {
    const meta = await ruleMetadata();
    const byBrowser = new Map();
    for (const rule of meta.filter((r) => r.browser)) {
      const key = rule.browserId;
      if (!byBrowser.has(key)) {
        byBrowser.set(key, {
          id: key,
          name: rule.browser,
          installed: false,
          categories: [],
        });
      }
      const entry = byBrowser.get(key);
      if (rule.applicable) entry.installed = true;
      entry.categories.push({
        id: rule.id,
        key: rule.id.split('.').pop(),
        name: rule.name,
        desc: rule.desc,
        default: rule.default,
        warning: rule.warning,
        applicable: rule.applicable,
      });
    }
    return { browsers: [...byBrowser.values()] };
  },

  'GET /api/disk': async (body, query) => {
    const target = query.get('path') || 'C:\\';
    const depth = Number(query.get('depth') || 2);
    return tools.analyzeDisk(target, { depth });
  },

  'POST /api/duplicates': async (body) => {
    const roots = (body.roots || []).filter(Boolean);
    if (!roots.length) throw new Error('Choose at least one folder to scan.');
    for (const root of roots) {
      if (!fs.existsSync(root)) throw new Error(`Folder not found: ${root}`);
    }
    return tools.findDuplicates(roots, { minSize: body.minSize, maxDepth: body.maxDepth });
  },

  'POST /api/files/delete': async (body) => {
    requireConfirm(body);
    const paths = Array.isArray(body.paths) ? body.paths : [];
    const roots = (Array.isArray(body.root) ? body.root : [body.root]).filter(Boolean);
    if (!paths.length) throw new Error('Nothing selected.');
    if (!roots.length) throw new Error('A scan root is required for safety.');

    let bytes = 0;
    let removed = 0;
    let skipped = 0;
    const errors = [];
    const settings = store.settings();

    for (const target of paths) {
      const res = await scanner.removePath(target, {
        secure: !!body.secure,
        extraRoots: roots,
        exclusions: settings.exclusions || [],
      });
      bytes += res.bytes;
      removed += res.removed;
      skipped += res.skipped;
      if (res.reason && res.reason !== 'excluded') errors.push(`${path.basename(target)}: ${res.reason}`);
      if (res.skipped && !res.reason) errors.push(`${path.basename(target)}: in use`);
    }

    if (removed > 0) store.recordClean({ bytes, files: removed, label: body.label || 'File removal' });
    return { bytes, removed, skipped, errors: errors.slice(0, 10), stats: store.stats() };
  },

  'POST /api/recyclebin/empty': async (body) => {
    requireConfirm(body);
    const before = await win.recycleBin();
    const result = await win.emptyRecycleBin();
    if (!result.ok) throw new Error(result.error || 'Could not empty the Recycle Bin.');
    if (before && before.count) {
      store.recordClean({ bytes: before.size || 0, files: before.count, label: 'Recycle Bin' });
    }
    return { ok: true, freed: before ? before.size : 0, count: before ? before.count : 0 };
  },

  'GET /api/uninstall': async () => ({ items: await win.installedSoftware() }),

  'POST /api/uninstall': async (body) => {
    requireConfirm(body);
    const items = await win.installedSoftware();
    const entry = items.find((i) => i.id === body.id);
    if (!entry) throw new Error('That program is no longer installed.');
    if (!entry.uninstall) throw new Error('This program has no uninstall command registered.');
    await win.launchUninstaller(entry.uninstall);
    return { ok: true, name: entry.name, message: `The uninstaller for ${entry.name} is starting. Follow its prompts to finish.` };
  },

  'GET /api/startup': async () => ({ items: await win.startupItems() }),

  'POST /api/startup/toggle': async (body) => {
    const item = body.item;
    if (!item || !item.name) throw new Error('No startup item supplied.');
    if (body.enabled === undefined) throw new Error('Missing target state.');
    await win.setStartupEnabled(item, !!body.enabled);
    return { ok: true, items: await win.startupItems() };
  },

  'POST /api/startup/delete': async (body) => {
    requireConfirm(body);
    const item = body.item;
    if (!item || !item.name) throw new Error('No startup item supplied.');
    const res = await win.deleteStartupItem(item);
    return { ...res, items: await win.startupItems() };
  },

  'GET /api/processes': async () => ({ items: await win.processes() }),

  'POST /api/processes/kill': async (body) => {
    requireConfirm(body);
    await win.killProcess(body.pid);
    return { ok: true };
  },

  'GET /api/software': async () => {
    const items = await win.installedSoftware();
    return { items, count: items.length };
  },

  'GET /api/software/updates': async () => win.wingetUpgrades(),

  'POST /api/software/upgrade': async (body) => {
    requireConfirm(body);
    if (!body.id) throw new Error('Missing package id.');
    const res = await win.run(
      'winget.exe',
      [
        'upgrade',
        '--id', String(body.id),
        '--silent',
        '--accept-package-agreements',
        '--accept-source-agreements',
        '--disable-interactivity',
      ],
      { timeout: 300000 }
    );
    const output = (res.stdout + '\n' + res.stderr).trim().split(/\r?\n/).slice(-12).join('\n');
    if (!res.ok) throw new Error(output || 'winget could not complete the upgrade.');
    return { ok: true, output };
  },

  'GET /api/drivers': async () => {
    const items = await win.drivers();
    const cutoff = Date.now() - 3 * 365 * 24 * 3600 * 1000;
    return {
      items: items.map((d) => ({ ...d, old: !!d.date && new Date(d.date).getTime() < cutoff })),
      old: items.filter((d) => d.date && new Date(d.date).getTime() < cutoff).length,
      total: items.length,
    };
  },

  'POST /api/restorepoint': async (body) => {
    requireConfirm(body);
    return win.restorePoint(body.label || 'OpenClean');
  },

  'GET /api/tasks': async () => ({ items: await win.scheduledTasks() }),

  'GET /api/cloud': async () => ({ drives: await tools.cloudDrives() }),

  'POST /api/cloud/scan': async (body) => {
    if (!body.path) throw new Error('Choose a cloud folder first.');
    return tools.cloudClutter(body.path);
  },

  'GET /api/stats': async () => store.stats(),

  'POST /api/stats/reset': async (body) => {
    requireConfirm(body);
    return store.resetStats();
  },

  'GET /api/settings': async () => store.settings(),

  'POST /api/settings': async (body) => {
    const patch = body && body.patch ? body.patch : body || {};
    const allowed = ['theme', 'confirmDeletes', 'secureDelete', 'exclusions'];
    const clean = {};
    for (const key of allowed) {
      if (patch[key] !== undefined) clean[key] = patch[key];
    }
    if (clean.exclusions && !Array.isArray(clean.exclusions)) clean.exclusions = [];
    ruleMetaCache = null;
    return store.updateSettings(clean);
  },

  'GET /api/paths': async () => ({
    home: process.env.USERPROFILE || '',
    temp: process.env.TEMP || '',
    localAppData: process.env.LOCALAPPDATA || '',
    appData: process.env.APPDATA || '',
    programData: process.env.ProgramData || '',
    windows: process.env.windir || 'C:\\Windows',
    drives: await win.drives(),
  }),
};

async function runClean(ruleIds, opts, label) {
  const ids = Array.isArray(ruleIds) ? ruleIds : [];
  if (!ids.length) throw new Error('Select at least one item to clean.');
  const selected = ids.map(getRule);

  let bytes = 0;
  let removed = 0;
  let skipped = 0;
  const results = [];

  for (const rule of selected) {
    const outcome = await scanner.cleanRule(rule, opts);
    bytes += outcome.bytes;
    removed += outcome.removed;
    skipped += outcome.skipped;
    results.push(outcome);
  }

  if (removed > 0 || bytes > 0) {
    store.recordClean({
      bytes,
      files: removed,
      label,
      details: results.filter((r) => r.removed > 0).map((r) => ({ name: r.name, bytes: r.bytes, removed: r.removed })),
    });
  }

  const actionErrors = results.flatMap((r) => r.actions.filter((a) => !a.ok).map((a) => a.error)).filter(Boolean);

  return {
    bytes,
    removed,
    skipped,
    results: results.map((r) => ({
      id: r.id,
      name: r.name,
      bytes: r.bytes,
      removed: r.removed,
      skipped: r.skipped,
      actions: r.actions,
    })),
    errors: actionErrors.slice(0, 8),
    stats: store.stats(),
  };
}

function formatBytes(bytes) {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(units.length - 1, Math.floor(Math.log(Math.max(bytes, 1)) / Math.log(1024)));
  const value = bytes / 1024 ** i;
  return `${value >= 100 || i === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[i]}`;
}

async function route(method, pathname, query, body) {
  const key = `${method} ${pathname}`;
  const handler = handlers[key];
  if (!handler) {
    const err = new Error(`No such endpoint: ${key}`);
    err.status = 404;
    throw err;
  }
  return handler(body || {}, query);
}

module.exports = { route, health };
