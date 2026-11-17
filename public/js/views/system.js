/* Statistics and Settings. */

import {
  $, api, dialog, esc, fmtBytes, fmtCount, icon, loadingBlock, pill, setBusy, setTheme, theme, timeAgo, toast,
} from '../core.js';

/* -------------------------------------------------------------- Statistics */

function paintStats(root, stats) {
  const history = stats.history || [];
  const average = stats.runs ? stats.lifetimeBytes / stats.runs : 0;
  const best = history.reduce((max, run) => Math.max(max, run.bytes || 0), 0);
  const maxRun = Math.max(1, ...history.map((h) => h.bytes || 0));

  root.innerHTML = `
    <div class="stack">
      <div class="grid grid-4">
        <div class="stat"><span class="stat-label">Lifetime freed</span><strong class="stat-value">${fmtBytes(stats.lifetimeBytes)}</strong><span class="stat-hint">across all clean-ups</span></div>
        <div class="stat"><span class="stat-label">Files removed</span><strong class="stat-value">${fmtCount(stats.lifetimeFiles)}</strong><span class="stat-hint">temporary and trace files</span></div>
        <div class="stat"><span class="stat-label">Clean-ups run</span><strong class="stat-value">${fmtCount(stats.runs)}</strong><span class="stat-hint">${history.length ? `last ${timeAgo(history[0].at)}` : 'none yet'}</span></div>
        <div class="stat"><span class="stat-label">Average per run</span><strong class="stat-value">${fmtBytes(average)}</strong><span class="stat-hint">${best ? `best run ${fmtBytes(best)}` : 'no data yet'}</span></div>
      </div>

      <section class="card">
        <div class="card-head">
          <div><h2>Recent clean-ups</h2><p>The last ${Math.min(history.length, 60)} recorded runs on this PC.</p></div>
          <div class="card-actions">
            ${history.length ? `<button class="btn btn-sm btn-ghost" data-act="reset" type="button">Reset statistics</button>` : ''}
          </div>
        </div>
        <div class="card-body ${history.length ? 'flush' : ''}">
          ${history.length ? `
            <div class="table-scroll" style="max-height:52vh">
              <table class="table">
                <thead><tr><th>When</th><th>What</th><th>Files</th><th class="num">Freed</th><th style="width:140px"></th></tr></thead>
                <tbody>
                  ${history.map((run) => `
                    <tr>
                      <td class="nowrap muted">${esc(timeAgo(run.at))}</td>
                      <td class="truncate" style="max-width:240px">${esc(run.label || 'Clean')}</td>
                      <td class="muted nowrap">${fmtCount(run.files)}</td>
                      <td class="num nowrap"><strong>${fmtBytes(run.bytes)}</strong></td>
                      <td><div class="bar"><span style="width:${Math.max(2, (run.bytes / maxRun) * 100).toFixed(1)}%"></span></div></td>
                    </tr>`).join('')}
                </tbody>
              </table>
            </div>` : `<div class="card-body">${emptyBlock('Nothing recorded yet', 'Run a Health Check or a clean-up and it will show up here.', 'stats')}</div>`}
        </div>
      </section>

      <div class="foot-note">Statistics stay on this machine in <code>data/store.json</code>. Nothing is uploaded, ever.</div>
    </div>`;

  const reset = root.querySelector('[data-act="reset"]');
  if (reset) reset.addEventListener('click', async () => {
    const ok = await dialog({
      title: 'Reset statistics?',
      body: `<p>Lifetime totals and the run history will be cleared. This cannot be undone.</p>`,
      confirmText: 'Reset',
      danger: true,
    });
    if (!ok) return;
    setBusy(reset, true, 'Resetting…');
    try {
      const fresh = await api('/stats/reset', { method: 'POST', body: { confirm: true } });
      toast({ title: 'Statistics reset', kind: 'good' });
      paintStats(root, fresh);
    } catch (err) {
      toast({ title: 'Could not reset', body: err.message, kind: 'bad' });
      setBusy(reset, false);
    }
  });
}

export const statsView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Loading statistics…')}</div></section>`;
    try {
      paintStats(root, await api('/stats'));
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};

/* ---------------------------------------------------------------- Settings */

const settings = { values: null, saving: false };

function exclusionRow(path) {
  return `
    <div class="row">
      <div class="row-main">
        <strong class="mono" style="font-weight:500;font-size:12.5px">${esc(path)}</strong>
        <small>Ignored by every scan and clean-up</small>
      </div>
      <div class="row-end"><button class="btn btn-sm btn-ghost" data-unexclude="${esc(path)}" type="button">Remove</button></div>
    </div>`;
}

function paintSettings(root) {
  const s = settings.values;
  if (!s) return;

  root.innerHTML = `
    <div class="stack">
      <section class="card">
        <div class="card-head"><div><h2>Appearance</h2><p>Pick the theme you prefer. The choice is remembered.</p></div></div>
        <div class="card-body">
          <div class="segmented">
            ${[['dark', 'Dark'], ['light', 'Light']].map(([id, label]) =>
              `<button data-theme-choice="${id}" type="button" class="${theme() === id ? 'active' : ''}">${label}</button>`).join('')}
          </div>
        </div>
      </section>

      <section class="card">
        <div class="card-head"><div><h2>Cleaning behaviour</h2><p>Controls that apply to every clean-up.</p></div></div>
        <div class="card-body">
          <div class="rows">
            <div class="row">
              <div class="row-main">
                <strong>Overwrite files before deleting</strong>
                <small>Writes random data over the first 8 MB of each file, then removes it. Slower, harder to recover.</small>
              </div>
              <div class="row-end">
                <label class="toggle"><input type="checkbox" data-flag="secureDelete" ${s.secureDelete ? 'checked' : ''}><span class="track"></span></label>
              </div>
            </div>
            <div class="row">
              <div class="row-main">
                <strong>Ask before removing anything</strong>
                <small>Every destructive action shows a confirmation with the exact size. Leave this on.</small>
              </div>
              <div class="row-end">
                <label class="toggle"><input type="checkbox" data-flag="confirmDeletes" ${s.confirmDeletes ? 'checked' : ''}><span class="track"></span></label>
              </div>
            </div>
          </div>
          <div class="inline-note" style="margin-top:14px">${icon('lock')}<div>Confirmations are enforced by the server: a request without an explicit confirmation token is rejected regardless of this switch.</div></div>
        </div>
      </section>

      <section class="card">
        <div class="card-head">
          <div><h2>Exclusions</h2><p>Paths listed here are never scanned or deleted, including everything inside them.</p></div>
          <div class="card-actions"><span class="tiny muted">${(s.exclusions || []).length} paths</span></div>
        </div>
        <div class="card-body">
          <div class="row-flex" style="margin-bottom:10px">
            <div class="search" style="flex:1;min-width:260px">
              ${icon('folder')}
              <input type="text" id="excl-input" placeholder="C:\\Users\\you\\AppData\\Local\\Something" />
            </div>
            <button class="btn btn-sm btn-primary" data-act="add-exclusion" type="button">Add path</button>
          </div>
          ${(s.exclusions || []).length ? (s.exclusions || []).map(exclusionRow).join('') : `<div class="tiny muted">No exclusions. Everything in the cache locations is fair game.</div>`}
        </div>
      </section>

      <section class="card">
        <div class="card-head"><div><h2>About OpenClean</h2><p>Version 1.0.0 · public domain</p></div></div>
        <div class="card-body">
          <div class="rows">
            <div class="row"><div class="row-main"><strong>Licence</strong><small>Released into the public domain (Unlicense). Use it, fork it, ship it — no attribution required.</small></div><div class="row-end">${pill('good', 'Open source')}</div></div>
            <div class="row"><div class="row-main"><strong>Activation</strong><small>There is none. Every feature is unlocked, permanently, with no account.</small></div><div class="row-end">${pill('neutral', 'No keys')}</div></div>
            <div class="row"><div class="row-main"><strong>Telemetry</strong><small>No analytics, no offers, no network calls. The server only listens on 127.0.0.1.</small></div><div class="row-end">${pill('neutral', 'None')}</div></div>
            <div class="row"><div class="row-main"><strong>Safety model</strong><small>Deletions are restricted to disposable cache roots and never follow junctions or symlinks.</small></div><div class="row-end">${pill('accent', 'Enforced')}</div></div>
          </div>
        </div>
      </section>
    </div>`;

  wireSettings(root);
}

async function patch(root, values) {
  settings.values = { ...settings.values, ...values };
  paintSettings(root);
  try {
    settings.values = await api('/settings', { method: 'POST', body: { patch: values } });
  } catch (err) {
    toast({ title: 'Could not save', body: err.message, kind: 'bad' });
  }
}

function wireSettings(root) {
  root.querySelectorAll('[data-theme-choice]').forEach((btn) => {
    btn.addEventListener('click', () => {
      setTheme(btn.dataset.themeChoice);
      patch(root, { theme: btn.dataset.themeChoice });
    });
  });

  root.querySelectorAll('[data-flag]').forEach((box) => {
    box.addEventListener('change', () => patch(root, { [box.dataset.flag]: box.checked }));
  });

  const addBtn = root.querySelector('[data-act="add-exclusion"]');
  const input = root.querySelector('#excl-input');
  const add = () => {
    const value = input.value.trim();
    if (!value) return;
    const list = [...(settings.values.exclusions || [])];
    if (list.includes(value)) {
      toast({ title: 'Already excluded', kind: 'info' });
      return;
    }
    list.push(value);
    patch(root, { exclusions: list });
    toast({ title: 'Exclusion added', kind: 'good' });
  };
  addBtn.addEventListener('click', add);
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') add(); });

  root.querySelectorAll('[data-unexclude]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const list = (settings.values.exclusions || []).filter((p) => p !== btn.dataset.unexclude);
      patch(root, { exclusions: list });
    });
  });
}

export const settingsView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Loading settings…')}</div></section>`;
    try {
      settings.values = await api('/settings');
      paintSettings(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};
