/* Custom Clean, Browser Cleaner and Cloud Drive Cleaner. */

import {
  $, api, dialog, emptyBlock, esc, fmtBytes, fmtCount, icon, loadingBlock, pill, setBusy, shortPath, toast,
} from '../core.js';

/* ------------------------------------------------------------ Custom Clean */

const ACTION_LABELS = {
  clipboard: 'Clear clipboard',
  dns: 'Flush DNS cache',
  recyclebin: 'Empty Recycle Bin',
};

const clean = {
  rules: [],
  groups: [],
  selected: new Set(),
  results: new Map(),
  collapsed: new Set(['Browser cleaner']),
  query: '',
  scanning: false,
  cleaning: false,
  loaded: false,
};

async function loadRules() {
  if (clean.loaded) return;
  const data = await api('/rules');
  clean.rules = data.rules;
  clean.groups = data.groups;
  clean.selected = new Set(data.rules.filter((r) => r.applicable && r.default).map((r) => r.id));
  clean.loaded = true;
}

function visibleRules() {
  const q = clean.query.trim().toLowerCase();
  let list = clean.rules.filter((r) => r.applicable);
  if (q) list = list.filter((r) => r.name.toLowerCase().includes(q) || r.group.toLowerCase().includes(q) || (r.browser || '').toLowerCase().includes(q));
  return list;
}

function cleanGroupBlock(group) {
  const rules = visibleRules().filter((r) => r.group === group);
  if (!rules.length) return '';
  const collapsed = clean.collapsed.has(group) && !clean.query;
  const allOn = rules.every((r) => clean.selected.has(r.id));
  const someOn = rules.some((r) => clean.selected.has(r.id));
  const size = rules.reduce((sum, r) => sum + (clean.results.get(r.id)?.size || 0), 0);
  const count = rules.reduce((sum, r) => sum + (clean.results.get(r.id)?.count || 0), 0);

  return `
    <div class="group-block" data-group="${esc(group)}">
      <div class="group-head">
        <label class="check"><input type="checkbox" data-group-toggle="${esc(group)}" ${allOn ? 'checked' : ''} data-indeterminate="${!allOn && someOn}"><span></span></label>
        <span>${esc(group)}</span>
        <span class="tiny muted">${rules.length} categories</span>
        <div class="bulk">
          ${clean.results.size ? `<span class="tiny muted">${fmtBytes(size)} · ${fmtCount(count)} files</span>` : ''}
          <button class="btn-link" data-collapse="${esc(group)}" type="button">${collapsed ? 'Show' : 'Hide'}</button>
        </div>
      </div>
      <div class="group-rows" ${collapsed ? 'hidden' : ''}>
        ${rules.map((rule) => {
          const res = clean.results.get(rule.id);
          return `
          <label class="pick-row" data-rule="${esc(rule.id)}">
            <span class="check"><input type="checkbox" data-rule-toggle="${esc(rule.id)}" ${clean.selected.has(rule.id) ? 'checked' : ''}><span></span></span>
            <div class="pick-main">
              <strong>${esc(rule.name)}${rule.warning ? ` ${pill('warn', 'sign-out')}` : ''}${rule.permanent ? ` ${pill('bad', 'permanent')}` : ''}</strong>
              <small>${esc(rule.desc || '')}</small>
            </div>
            <div class="row-end ${res && res.size ? 'strong' : ''}">
              ${res ? (res.size ? fmtBytes(res.size) : res.actions && res.actions.length ? 'action' : '—') : ''}
            </div>
          </label>`;
        }).join('')}
      </div>
    </div>`;
}

function selectedSize() {
  let size = 0;
  for (const id of clean.selected) size += clean.results.get(id)?.size || 0;
  return size;
}

function paintClean(root) {
  const rules = visibleRules();
  const groups = clean.groups.filter((g) => rules.some((r) => r.group === g));

  root.innerHTML = `
    <div class="stack">
      <section class="card">
        <div class="list-toolbar">
          <div class="search">
            ${icon('search')}
            <input type="search" id="rule-search" placeholder="Search categories" value="${esc(clean.query)}" />
          </div>
          <button class="btn btn-sm btn-ghost" data-act="select-defaults" type="button">Defaults</button>
          <button class="btn btn-sm btn-ghost" data-act="select-none" type="button">None</button>
          <button class="btn btn-sm btn-ghost" data-act="select-all" type="button">All</button>
          <div class="spacer"></div>
          <button class="btn btn-sm btn-ghost" data-act="analyze" type="button" ${clean.scanning ? 'disabled' : ''}>
            ${icon('search')} Analyze
          </button>
        </div>

        <div class="table-scroll" style="max-height:58vh">
          ${groups.length ? groups.map(cleanGroupBlock).join('') : emptyBlock('No categories match', 'Try a different search term.', 'search')}
        </div>

        <div class="result-summary">
          <div>
            <div class="rs-value">${fmtBytes(selectedSize())}</div>
            <div class="tiny muted">${clean.selected.size} categories selected${clean.results.size ? ` · ${fmtCount([...clean.selected].reduce((n, id) => n + (clean.results.get(id)?.count || 0), 0))} files found` : ''}</div>
          </div>
          <div class="spacer"></div>
          <button class="btn btn-primary" data-act="clean" type="button" ${!clean.selected.size || clean.cleaning || !clean.results.size ? 'disabled' : ''}>
            ${icon('check')} Clean selected
          </button>
        </div>
      </section>

      <div class="foot-note">
        Analysis only measures. Removal happens after you confirm, and OpenClean refuses to delete anything outside
        disposable cache locations. Files in use are skipped rather than forced.
      </div>
    </div>`;

  wireClean(root);
}

function wireClean(root) {
  const search = root.querySelector('#rule-search');
  search.addEventListener('input', (e) => {
    clean.query = e.target.value;
    const caret = e.target.selectionStart;
    paintClean(root);
    const next = root.querySelector('#rule-search');
    next.focus();
    next.setSelectionRange(caret, caret);
  });

  root.querySelectorAll('[data-rule-toggle]').forEach((box) => {
    box.addEventListener('change', () => {
      if (box.checked) clean.selected.add(box.dataset.ruleToggle);
      else clean.selected.delete(box.dataset.ruleToggle);
      paintClean(root);
    });
  });

  root.querySelectorAll('[data-group-toggle]').forEach((box) => {
    if (box.dataset.indeterminate === 'true') box.indeterminate = true;
    box.addEventListener('change', () => {
      const group = box.dataset.groupToggle;
      const rules = visibleRules().filter((r) => r.group === group);
      for (const rule of rules) {
        if (box.checked) clean.selected.add(rule.id);
        else clean.selected.delete(rule.id);
      }
      paintClean(root);
    });
  });

  root.querySelectorAll('[data-collapse]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      const group = btn.dataset.collapse;
      if (clean.collapsed.has(group)) clean.collapsed.delete(group);
      else clean.collapsed.add(group);
      paintClean(root);
    });
  });

  const acts = {
    'select-defaults': () => {
      clean.selected = new Set(clean.rules.filter((r) => r.applicable && r.default).map((r) => r.id));
    },
    'select-none': () => { clean.selected = new Set(); },
    'select-all': () => {
      clean.selected = new Set(visibleRules().map((r) => r.id));
    },
  };
  for (const [act, fn] of Object.entries(acts)) {
    const btn = root.querySelector(`[data-act="${act}"]`);
    if (btn) btn.addEventListener('click', () => { fn(); paintClean(root); });
  }

  root.querySelector('[data-act="analyze"]').addEventListener('click', (e) => analyze(e.currentTarget));
  const cleanBtn = root.querySelector('[data-act="clean"]');
  if (cleanBtn) cleanBtn.addEventListener('click', () => runClean(cleanBtn));
}

async function analyze(button) {
  const ids = [...clean.selected];
  if (!ids.length) {
    toast({ title: 'Nothing selected', body: 'Pick at least one category first.', kind: 'warn' });
    return;
  }
  clean.scanning = true;
  setBusy(button, true, 'Analyzing…');
  try {
    const res = await api('/rules/scan', { method: 'POST', body: { ruleIds: ids } });
    clean.results = new Map(res.results.map((r) => [r.id, r]));
    const total = res.results.reduce((s, r) => s + r.size, 0);
    const count = res.results.reduce((s, r) => s + r.count, 0);
    toast({ title: `Found ${fmtBytes(total)}`, body: `${fmtCount(count)} files across ${res.results.filter((r) => r.count > 0).length} categories.`, kind: total ? 'good' : 'info' });
  } catch (err) {
    toast({ title: 'Analysis failed', body: err.message, kind: 'bad' });
  } finally {
    clean.scanning = false;
    paintClean($('#view'));
  }
}

async function runClean(button) {
  // Action-only rules (Recycle Bin, DNS cache, clipboard) have no bytes on
  // disk, but still have work to do — they must not be filtered out here.
  const ids = [...clean.selected].filter((id) => {
    const result = clean.results.get(id);
    return result && (result.size > 0 || (result.actions || []).length > 0);
  });
  if (!ids.length) {
    toast({ title: 'Nothing to clean', body: 'Nothing in the selected categories was found.', kind: 'info' });
    return;
  }

  const chosenRules = ids.map((id) => clean.rules.find((r) => r.id === id)).filter(Boolean);
  const permanent = chosenRules.filter((r) => r.permanent).map((r) => r.name);
  const actions = [...new Set(ids.flatMap((id) => clean.results.get(id)?.actions || []))].map(
    (a) => ACTION_LABELS[a] || a
  );

  const body = `
    <p>OpenClean will remove <strong>${fmtBytes(selectedSize())}</strong> from ${ids.length} categories.</p>
    ${actions.length ? `<p style="margin-top:10px">System actions included: <strong>${actions.map(esc).join('</strong>, <strong>')}</strong>.</p>` : ''}
    ${permanent.length
      ? `<div class="inline-note bad" style="margin-top:12px">${icon('alert')}<div><strong>${permanent.map(esc).join(', ')} destroys files permanently.</strong> Anything in there cannot be restored afterwards. Untick it if you are not certain.</div></div>`
      : `<div class="inline-note accent" style="margin-top:12px">${icon('shield')}<div>Only cache and trace locations are touched. Files in use are skipped.</div></div>`}`;

  const ok = await dialog({
    title: 'Clean the selected categories?',
    body,
    confirmText: selectedSize() ? `Free ${fmtBytes(selectedSize())}` : 'Clean',
    danger: permanent.length > 0,
  });
  if (!ok) return;

  clean.cleaning = true;
  setBusy(button, true, 'Cleaning…');
  try {
    const res = await api('/rules/clean', { method: 'POST', body: { ruleIds: ids, confirm: true, label: 'Custom Clean' } });
    toast({
      title: `Freed ${fmtBytes(res.bytes)}`,
      body: `${fmtCount(res.removed)} files removed${res.skipped ? ` · ${fmtCount(res.skipped)} in use` : ''}.`,
      kind: 'good',
    });
    clean.results = new Map();
    await loadRules();
  } catch (err) {
    toast({ title: 'Clean failed', body: err.message, kind: 'bad' });
  } finally {
    clean.cleaning = false;
    paintClean($('#view'));
  }
}

export const cleanView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Loading cleaning rules…')}</div></section>`;
    try {
      await loadRules();
      paintClean(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};

/* --------------------------------------------------------- Browser Cleaner */

const browser = { browsers: [], data: new Map(), busy: false, loaded: false };

async function loadBrowsers() {
  const res = await api('/browsers');
  browser.browsers = res.browsers;
  if (!browser.data.size) {
    for (const b of res.browsers) {
      browser.data.set(b.id, { selected: new Set(b.categories.filter((c) => c.default && c.applicable).map((c) => c.id)), results: null });
    }
  }
  browser.loaded = true;
}

function browserCard(b) {
  const state = browser.data.get(b.id);
  const detected = b.categories.some((c) => c.applicable);
  const size = state.results ? state.results.reduce((s, r) => s + r.size, 0) : 0;
  const count = state.results ? state.results.reduce((s, r) => s + r.count, 0) : 0;

  return `
    <section class="card" data-browser="${esc(b.id)}" style="animation-delay:${Math.random() * 60}ms">
      <div class="card-head">
        <div>
          <h2>${esc(b.name)}</h2>
          <p>${detected ? (state.results ? `${fmtBytes(size)} · ${fmtCount(count)} items found` : 'Installed · ready to analyze') : 'Not installed'}</p>
        </div>
        <div class="card-actions">
          ${detected ? pill('accent', 'Installed') : pill('neutral', 'Not found')}
        </div>
      </div>
      <div class="card-body tight">
        ${b.categories.filter((c) => c.applicable).map((c) => {
          const res = state.results ? state.results.find((r) => r.id === c.id) : null;
          return `
            <label class="pick-row" style="padding-left:10px;padding-right:10px">
              <span class="check"><input type="checkbox" data-bcat="${esc(b.id)}|${esc(c.id)}" ${state.selected.has(c.id) ? 'checked' : ''}><span></span></span>
              <div class="pick-main">
                <strong>${esc(c.name)}${c.warning ? ` ${pill('warn', 'logs you out')}` : ''}</strong>
                <small>${esc(c.desc || '')}</small>
              </div>
              <div class="row-end ${res && res.size ? 'strong' : ''}">${res ? (res.size ? fmtBytes(res.size) : '—') : ''}</div>
            </label>`;
        }).join('') || emptyBlock('Browser data not found')}
      </div>
      ${detected ? `
      <div class="result-summary">
        <span class="tiny muted">${state.selected.size} categories selected</span>
        <div class="spacer"></div>
        <button class="btn btn-sm btn-ghost" data-bact="analyze" data-browser="${esc(b.id)}" type="button">${icon('search')} Analyze</button>
        <button class="btn btn-sm btn-primary" data-bact="clean" data-browser="${esc(b.id)}" type="button" ${!state.selected.size ? 'disabled' : ''}>${icon('check')} Clean</button>
      </div>` : ''}
    </section>`;
}

function paintBrowsers(root) {
  root.innerHTML = `
    <div class="stack">
      <div class="inline-note">${icon('info')}<div>Browsers must be closed before their databases can be cleared. OpenClean skips any file that is locked, so a running browser simply keeps its current session.</div></div>
      <div class="grid grid-2">${browser.browsers.map(browserCard).join('')}</div>
    </div>`;
  wireBrowsers(root);
}

function wireBrowsers(root) {
  root.querySelectorAll('[data-bcat]').forEach((box) => {
    box.addEventListener('change', () => {
      const [bid, cid] = box.dataset.bcat.split('|');
      const entry = browser.data.get(bid);
      if (box.checked) entry.selected.add(cid);
      else entry.selected.delete(cid);
      paintBrowsers(root);
    });
  });

  root.querySelectorAll('[data-bact]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const id = btn.dataset.browser;
      if (btn.dataset.bact === 'analyze') analyzeBrowser(id, btn);
      else cleanBrowser(id, btn);
    });
  });
}

function idsFor(id) {
  const entry = browser.data.get(id);
  return [...entry.selected];
}

async function analyzeBrowser(id, button) {
  const ids = idsFor(id);
  if (!ids.length) return;
  setBusy(button, true, 'Analyzing…');
  try {
    const res = await api('/rules/scan', { method: 'POST', body: { ruleIds: ids } });
    const entry = browser.data.get(id);
    entry.results = res.results;
    paintBrowsers($('#view'));
  } catch (err) {
    toast({ title: 'Analysis failed', body: err.message, kind: 'bad' });
    setBusy(button, false);
  }
}

async function cleanBrowser(id, button) {
  const entry = browser.data.get(id);
  const name = browser.browsers.find((b) => b.id === id)?.name || 'browser';
  const ids = [...entry.selected].filter((ruleId) => (entry.results?.find((r) => r.id === ruleId)?.size || 0) > 0);
  if (!ids.length) {
    const any = await api('/rules/scan', { method: 'POST', body: { ruleIds: idsFor(id) } });
    const total = any.results.reduce((s, r) => s + r.size, 0);
    if (!total) {
      toast({ title: 'Nothing to clean', body: `${name} has no removable data right now.`, kind: 'info' });
      return;
    }
    ids.push(...any.results.filter((r) => r.size > 0).map((r) => r.id));
  }

  const ok = await dialog({
    title: `Clean ${name}?`,
    body: `<p>The selected caches and traces will be removed. Cookies will sign you out of sites; saved passwords are excluded unless you ticked them.</p>`,
    confirmText: 'Clean',
  });
  if (!ok) return;

  setBusy(button, true, 'Cleaning…');
  try {
    const res = await api('/rules/clean', { method: 'POST', body: { ruleIds: ids, confirm: true, label: `${name} clean` } });
    toast({ title: `${name}: freed ${fmtBytes(res.bytes)}`, body: `${fmtCount(res.removed)} files removed.`, kind: 'good' });
    entry.results = null;
    paintBrowsers($('#view'));
  } catch (err) {
    toast({ title: 'Clean failed', body: err.message, kind: 'bad' });
    setBusy(button, false);
  }
}

export const browserView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Detecting installed browsers…')}</div></section>`;
    try {
      await loadBrowsers();
      paintBrowsers(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};

/* ----------------------------------------------------- Cloud Drive Cleaner */

const cloud = { drives: [], active: null, clutter: null, busy: false };

function cloudDriveCard(drive) {
  const active = cloud.active === drive.path;
  return `
    <section class="card" style="${active ? 'border-color:var(--accent)' : ''}">
      <div class="card-head">
        <div>
          <h2>${esc(drive.name)}</h2>
          <p class="mono">${esc(shortPath(drive.path, 54))}</p>
        </div>
        <div class="card-actions">${active ? pill('accent', 'Selected') : ''}</div>
      </div>
      <div class="card-body">
        <div class="row-flex" style="justify-content:space-between;align-items:flex-end">
          <div>
            <div class="stat-value">${fmtBytes(drive.size)}</div>
            <small class="muted">${fmtCount(drive.files)} files · ${drive.folders} folders${drive.partial ? ' · partial scan' : ''}</small>
          </div>
          <button class="btn btn-sm ${active ? 'btn-primary' : ''}" data-cloud-pick="${esc(drive.path)}" type="button">
            ${active ? 'Scanning folder…' : 'Find clutter'}
          </button>
        </div>
      </div>
    </section>`;
}

function clutterBlock() {
  if (!cloud.clutter) return '';
  const c = cloud.clutter;
  return `
    <section class="card">
      <div class="card-head">
        <div>
          <h2>Clutter in ${esc(shortPath(c.root, 46))}</h2>
          <p>${fmtBytes(c.totalSize)} scanned${c.truncated ? ' (partial)' : ''}. Tick what you want removed.</p>
        </div>
      </div>
      <div class="card-body tight">
        ${c.categories.map((cat) => `
          <div class="group-head" style="border-radius:8px">
            <span>${esc(cat.name)}</span>
            <div class="bulk"><span class="tiny muted">${cat.items.length} items · ${fmtBytes(cat.size)}</span></div>
          </div>
          ${cat.items.length ? cat.items.slice(0, 40).map((item) => `
            <label class="pick-row">
              <span class="check"><input type="checkbox" data-cloud-file="${esc(item.path)}" data-cloud-size="${item.size || 0}"><span></span></span>
              <div class="pick-main"><strong class="mono" style="font-weight:500">${esc(shortPath(item.path, 68))}</strong></div>
              <div class="row-end">${item.size ? fmtBytes(item.size) : 'empty'}</div>
            </label>`).join('') : `<div class="pick-row"><small class="muted">Nothing here.</small></div>`}
        `).join('')}
      </div>
      <div class="result-summary">
        <div class="rs-value" id="cloud-selected">0 B</div>
        <div class="spacer"></div>
        <button class="btn btn-sm btn-primary" data-act="cloud-delete" type="button">${icon('trash')} Delete selected</button>
      </div>
    </section>`;
}

function paintCloud(root) {
  root.innerHTML = `
    <div class="stack">
      ${cloud.drives.length
        ? `<div class="grid grid-2">${cloud.drives.map(cloudDriveCard).join('')}</div>`
        : `<section class="card"><div class="card-body">${emptyBlock('No synced cloud folders found', 'Install a desktop client for OneDrive, Dropbox, Google Drive, iCloud or Nextcloud and it will appear here.', 'cloud')}</div></section>`}
      ${clutterBlock()}
    </div>`;
  wireCloud(root);
}

function selectedCloudFiles(root) {
  return [...root.querySelectorAll('[data-cloud-file]:checked')].map((box) => ({
    path: box.dataset.cloudFile,
    size: Number(box.dataset.cloudSize) || 0,
  }));
}

function wireCloud(root) {
  root.querySelectorAll('[data-cloud-pick]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      cloud.active = btn.dataset.cloudPick;
      cloud.clutter = null;
      setBusy(btn, true, 'Scanning…');
      try {
        cloud.clutter = await api('/cloud/scan', { method: 'POST', body: { path: cloud.active } });
      } catch (err) {
        toast({ title: 'Scan failed', body: err.message, kind: 'bad' });
      }
      paintCloud(root);
    });
  });

  const updateTotal = () => {
    const files = selectedCloudFiles(root);
    const total = files.reduce((s, f) => s + f.size, 0);
    const el = root.querySelector('#cloud-selected');
    if (el) el.textContent = `${fmtBytes(total)} · ${files.length} selected`;
  };
  root.querySelectorAll('[data-cloud-file]').forEach((box) => box.addEventListener('change', updateTotal));

  const del = root.querySelector('[data-act="cloud-delete"]');
  if (del) del.addEventListener('click', async () => {
    const files = selectedCloudFiles(root);
    if (!files.length) {
      toast({ title: 'Nothing selected', kind: 'warn' });
      return;
    }
    const total = files.reduce((s, f) => s + f.size, 0);
    const ok = await dialog({
      title: 'Delete from the cloud folder?',
      body: `<p>${files.length} files (${fmtBytes(total)}) will be deleted. Your cloud client will sync the deletion to every device.</p>`,
      confirmText: 'Delete',
      danger: true,
    });
    if (!ok) return;
    setBusy(del, true, 'Deleting…');
    try {
      const res = await api('/files/delete', {
        method: 'POST',
        body: { paths: files.map((f) => f.path), root: cloud.active, confirm: true, label: 'Cloud Drive Cleaner' },
      });
      toast({ title: `Removed ${fmtCount(res.removed)} items`, body: `${fmtBytes(res.bytes)} freed.`, kind: 'good' });
      cloud.clutter = await api('/cloud/scan', { method: 'POST', body: { path: cloud.active } });
      paintCloud(root);
    } catch (err) {
      toast({ title: 'Delete failed', body: err.message, kind: 'bad' });
      setBusy(del, false);
    }
  });
}

export const cloudView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Looking for synced cloud folders…')}</div></section>`;
    try {
      const res = await api('/cloud');
      cloud.drives = res.drives;
      paintCloud(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};
