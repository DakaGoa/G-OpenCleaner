/* Startup Manager, Performance Optimizer and Uninstaller. */

import {
  $, api, dialog, emptyBlock, esc, fmtBytes, fmtCount, fmtDuration, icon, loadingBlock, pill, setBusy, shortPath, toast,
} from '../core.js';

/* --------------------------------------------------------- Startup Manager */

const startup = { items: [], filter: 'all', query: '', busy: false, admin: false };

function filteredStartup() {
  const q = startup.query.trim().toLowerCase();
  return startup.items.filter((item) => {
    if (startup.filter === 'enabled' && !item.enabled) return false;
    if (startup.filter === 'disabled' && item.enabled) return false;
    if (q && !`${item.name} ${item.command} ${item.source}`.toLowerCase().includes(q)) return false;
    return true;
  });
}

function startupRow(item) {
  return `
    <tr data-startup-id="${esc(item.id)}">
      <td style="width:52px">
        <label class="toggle" title="${item.kind === 'folder' ? 'Move the shortcut out of the startup folder to disable it' : item.enabled ? 'Disable at sign-in' : 'Enable at sign-in'}">
          <input type="checkbox" data-startup-toggle="${esc(item.id)}" ${item.enabled ? 'checked' : ''} ${startup.busy || item.kind === 'folder' ? 'disabled' : ''}>
          <span class="track"></span>
        </label>
      </td>
      <td>
        <strong style="font-size:13px">${esc(item.name)}</strong>
        <div class="tiny muted">${esc(item.source)}</div>
      </td>
      <td class="truncate mono tiny muted" title="${esc(item.command)}" style="max-width:340px">${esc(shortPath(item.command, 62))}</td>
      <td class="nowrap">${item.enabled ? pill('good', 'Enabled') : pill('neutral', 'Disabled')}</td>
      <td class="nowrap" style="text-align:right">
        <button class="btn btn-sm btn-ghost" data-startup-delete="${esc(item.id)}" type="button">Remove</button>
      </td>
    </tr>`;
}

function paintStartup(root) {
  const list = filteredStartup();
  const enabled = startup.items.filter((i) => i.enabled).length;

  root.innerHTML = `
    <div class="stack">
      <div class="grid grid-3">
        <div class="stat"><span class="stat-label">Startup apps</span><strong class="stat-value">${startup.items.length}</strong><span class="stat-hint">registered entries</span></div>
        <div class="stat"><span class="stat-label">Running at sign-in</span><strong class="stat-value">${enabled}</strong><span class="stat-hint">${enabled > 6 ? 'this slows boot' : 'healthy range'}</span></div>
        <div class="stat"><span class="stat-label">Disabled</span><strong class="stat-value">${startup.items.length - enabled}</strong><span class="stat-hint">kept, easy to restore</span></div>
      </div>

      ${!startup.admin ? `<div class="inline-note warn">${icon('alert')}<div><strong>Standard user rights.</strong> Entries under HKLM (all users) cannot be changed without restarting OpenClean as administrator.</div></div>` : ''}

      <section class="card">
        <div class="list-toolbar">
          <div class="segmented">
            ${[['all', 'All'], ['enabled', 'Enabled'], ['disabled', 'Disabled']].map(([id, label]) =>
              `<button data-filter="${id}" type="button" class="${startup.filter === id ? 'active' : ''}">${label}</button>`).join('')}
          </div>
          <div class="search">
            ${icon('search')}
            <input type="search" id="startup-search" placeholder="Filter by name or command" value="${esc(startup.query)}" />
          </div>
        </div>
        <div class="card-body flush">
          ${list.length ? `
            <div class="table-scroll">
              <table class="table">
                <thead><tr><th></th><th>Program</th><th>Command</th><th>State</th><th></th></tr></thead>
                <tbody>${list.map(startupRow).join('')}</tbody>
              </table>
            </div>` : emptyBlock('Nothing to show', 'No startup entries match this filter.', 'rocket')}
        </div>
      </section>

      <div class="foot-note">Disabling moves the entry into a backup key instead of deleting it, so nothing is lost.</div>
    </div>`;

  wireStartup(root);
}

function wireStartup(root) {
  root.querySelectorAll('[data-filter]').forEach((btn) => {
    btn.addEventListener('click', () => { startup.filter = btn.dataset.filter; paintStartup(root); });
  });

  const search = root.querySelector('#startup-search');
  if (search) {
    search.addEventListener('input', (e) => {
      startup.query = e.target.value;
      paintStartup(root);
      const next = root.querySelector('#startup-search');
      next.focus();
      next.setSelectionRange(next.value.length, next.value.length);
    });
  }

  root.querySelectorAll('[data-startup-toggle]').forEach((box) => {
    box.addEventListener('change', async () => {
      const item = startup.items.find((i) => i.id === box.dataset.startupToggle);
      if (!item) return;
      startup.busy = true;
      box.disabled = true;
      try {
        const res = await api('/startup/toggle', { method: 'POST', body: { item, enabled: box.checked } });
        startup.items = res.items;
        toast({ title: box.checked ? `${item.name} enabled` : `${item.name} disabled`, kind: 'good' });
      } catch (err) {
        toast({ title: 'Could not change that entry', body: err.message, kind: 'bad' });
      } finally {
        startup.busy = false;
        paintStartup(root);
      }
    });
  });

  root.querySelectorAll('[data-startup-delete]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const item = startup.items.find((i) => i.id === btn.dataset.startupDelete);
      if (!item) return;
      const ok = await dialog({
        title: `Remove ${item.name}?`,
        body: `<p>This deletes the startup entry. The program itself stays installed, it simply stops launching on its own.</p>
               <div class="tiny muted mono" style="margin-top:8px;word-break:break-all">${esc(item.command)}</div>`,
        confirmText: 'Remove entry',
        danger: true,
      });
      if (!ok) return;
      setBusy(btn, true, 'Removing…');
      try {
        const res = await api('/startup/delete', { method: 'POST', body: { item, confirm: true } });
        startup.items = res.items;
        toast({ title: 'Startup entry removed', body: res.movedTo ? `Shortcut moved to ${res.movedTo}` : '', kind: 'good' });
        paintStartup(root);
      } catch (err) {
        toast({ title: 'Could not remove it', body: err.message, kind: 'bad' });
        setBusy(btn, false);
      }
    });
  });
}

export const startupView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Reading startup entries…')}</div></section>`;
    try {
      const [res, system] = await Promise.all([api('/startup'), api('/system')]);
      startup.items = res.items;
      startup.admin = !!system.admin;
      paintStartup(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};

/* ---------------------------------------------------- Performance Optimizer */

const perf = { items: [], tasks: [], sort: 'cpu', query: '', system: null, busy: false };

function sortedProcesses() {
  const q = perf.query.trim().toLowerCase();
  let list = perf.items.filter((p) => !q || p.name.toLowerCase().includes(q));
  const key = perf.sort;
  list = [...list].sort((a, b) => (key === 'name' ? a.name.localeCompare(b.name) : b[key] - a[key]));
  return list.slice(0, 120);
}

function paintPerf(root) {
  const list = sortedProcesses();
  const memTotal = perf.system?.memTotal || 0;
  const memFree = perf.system?.memFree || 0;
  const usedPct = memTotal ? ((memTotal - memFree) / memTotal) * 100 : 0;
  const hottest = perf.items[0];

  root.innerHTML = `
    <div class="stack">
      <div class="grid grid-3">
        <div class="stat"><span class="stat-label">Processes</span><strong class="stat-value">${fmtCount(perf.items.length)}</strong><span class="stat-hint">running right now</span></div>
        <div class="stat"><span class="stat-label">Memory in use</span><strong class="stat-value">${usedPct.toFixed(0)}%</strong><span class="stat-hint">${fmtBytes(memTotal - memFree)} of ${fmtBytes(memTotal)}</span></div>
        <div class="stat"><span class="stat-label">Uptime</span><strong class="stat-value">${fmtDuration(perf.system?.uptime || 0)}</strong><span class="stat-hint">since last reboot</span></div>
      </div>

      <section class="card">
        <div class="list-toolbar">
          <div class="search">
            ${icon('search')}
            <input type="search" id="proc-search" placeholder="Filter processes" value="${esc(perf.query)}" />
          </div>
          <div class="segmented">
            ${[['cpu', 'CPU'], ['memory', 'Memory'], ['name', 'Name']].map(([id, label]) =>
              `<button data-sort="${id}" type="button" class="${perf.sort === id ? 'active' : ''}">${label}</button>`).join('')}
          </div>
          <div class="spacer"></div>
          <button class="btn btn-sm btn-ghost" data-act="proc-refresh" type="button">${icon('refresh')} Refresh</button>
        </div>
        <div class="card-body flush">
          <div class="table-scroll" style="max-height:56vh">
            <table class="table">
              <thead><tr><th>Process</th><th class="num">PID</th><th class="num">CPU</th><th class="num">Memory</th><th></th></tr></thead>
              <tbody>
                ${list.map((p) => `
                  <tr>
                    <td class="truncate" style="max-width:320px"><strong style="font-size:13px">${esc(p.name)}</strong>
                      ${p.path ? `<div class="tiny muted truncate" title="${esc(p.path)}">${esc(shortPath(p.path, 52))}</div>` : ''}
                    </td>
                    <td class="num muted">${p.pid}</td>
                    <td class="num nowrap">${p.cpu.toFixed(1)}%</td>
                    <td class="num nowrap">${fmtBytes(p.memory)}</td>
                    <td style="text-align:right"><button class="btn btn-sm btn-ghost" data-kill="${p.pid}" data-name="${esc(p.name)}" type="button">End</button></td>
                  </tr>`).join('')}
              </tbody>
            </table>
          </div>
        </div>
      </section>

      <section class="card">
        <div class="card-head">
          <div><h2>Scheduled tasks</h2><p>Non-Microsoft tasks registered on this PC.</p></div>
          <div class="card-actions"><span class="tiny muted">${perf.tasks.length} tasks</span></div>
        </div>
        <div class="card-body">
          ${perf.tasks.length ? perf.tasks.slice(0, 40).map((task) => `
            <div class="row">
              <div class="row-main">
                <strong>${esc(task.name)}</strong>
                <small class="mono">${esc(task.path)}</small>
              </div>
              <div class="row-end">${task.state === 'Ready' ? pill('good', 'Ready') : pill('neutral', task.state || 'Unknown')}</div>
            </div>`).join('') : `<div class="tiny muted">No third-party scheduled tasks.</div>`}
        </div>
      </section>

      <div class="foot-note">${hottest ? `Busiest process: <strong>${esc(hottest.name)}</strong> at ${hottest.cpu.toFixed(1)}% CPU.` : ''} Ending a process discards its unsaved work.</div>
    </div>`;

  wirePerf(root);
}

function wirePerf(root) {
  root.querySelectorAll('[data-sort]').forEach((btn) => {
    btn.addEventListener('click', () => { perf.sort = btn.dataset.sort; paintPerf(root); });
  });

  const search = root.querySelector('#proc-search');
  if (search) {
    search.addEventListener('input', (e) => {
      perf.query = e.target.value;
      paintPerf(root);
      const next = root.querySelector('#proc-search');
      next.focus();
      next.setSelectionRange(next.value.length, next.value.length);
    });
  }

  root.querySelector('[data-act="proc-refresh"]').addEventListener('click', (e) => loadProcesses(e.currentTarget, root));

  root.querySelectorAll('[data-kill]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const name = btn.dataset.name;
      const ok = await dialog({
        title: `End ${name}?`,
        body: `<p>Unsaved work in this process will be lost. Windows restarts essential services on its own, but ending system processes can make the desktop unstable.</p>`,
        confirmText: 'End process',
        danger: true,
      });
      if (!ok) return;
      setBusy(btn, true, 'Ending…');
      try {
        await api('/processes/kill', { method: 'POST', body: { pid: Number(btn.dataset.kill), confirm: true } });
        toast({ title: `${name} ended`, kind: 'good' });
        await loadProcesses(null, root);
      } catch (err) {
        toast({ title: 'Could not end process', body: err.message, kind: 'bad' });
        setBusy(btn, false);
      }
    });
  });
}

async function loadProcesses(button, root) {
  if (button) setBusy(button, true, 'Sampling…');
  try {
    const res = await api('/processes');
    perf.items = res.items;
    paintPerf(root || $('#view'));
  } catch (err) {
    toast({ title: 'Could not read processes', body: err.message, kind: 'bad' });
    if (button) setBusy(button, false);
  }
}

export const performanceView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Sampling running processes…')}</div></section>`;
    try {
      const [procs, tasks, system] = await Promise.all([api('/processes'), api('/tasks'), api('/system')]);
      perf.items = procs.items;
      perf.tasks = tasks.items;
      perf.system = system;
      paintPerf(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};

/* ------------------------------------------------------------- Uninstaller */

const uninstall = { items: [], query: '', loaded: false };

function filteredApps() {
  const q = uninstall.query.trim().toLowerCase();
  if (!q) return uninstall.items;
  return uninstall.items.filter((a) => `${a.name} ${a.publisher}`.toLowerCase().includes(q));
}

function paintUninstall(root) {
  const list = filteredApps();
  const biggest = [...uninstall.items].sort((a, b) => b.size - a.size)[0];

  root.innerHTML = `
    <div class="stack">
      <section class="card">
        <div class="list-toolbar">
          <div class="search">
            ${icon('search')}
            <input type="search" id="app-search" placeholder="Search installed programs" value="${esc(uninstall.query)}" />
          </div>
          <div class="spacer"></div>
          <span class="tiny muted">${fmtCount(uninstall.items.length)} programs${biggest && biggest.size ? ` · largest is ${fmtBytes(biggest.size)}` : ''}</span>
        </div>
        <div class="card-body flush">
          ${list.length ? `
            <div class="table-scroll" style="max-height:64vh">
              <table class="table">
                <thead><tr><th>Program</th><th>Publisher</th><th>Version</th><th>Installed</th><th class="num">Size</th><th></th></tr></thead>
                <tbody>
                  ${list.map((app) => `
                    <tr>
                      <td class="truncate" style="max-width:300px"><strong style="font-size:13px">${esc(app.name)}</strong></td>
                      <td class="truncate muted" style="max-width:180px">${esc(app.publisher || '—')}</td>
                      <td class="muted nowrap">${esc(app.version || '—')}</td>
                      <td class="muted nowrap">${esc(app.installDate || '—')}</td>
                      <td class="num nowrap">${app.size ? fmtBytes(app.size) : '—'}</td>
                      <td style="text-align:right"><button class="btn btn-sm btn-ghost" data-uninstall="${esc(app.id)}" type="button">Uninstall</button></td>
                    </tr>`).join('')}
                </tbody>
              </table>
            </div>` : emptyBlock('No programs match', 'Clear the search to see everything.', 'box')}
        </div>
      </section>

      <div class="foot-note">OpenClean launches each program's own uninstaller. Leftover files stay your choice — nothing is force-deleted behind the vendor's back.</div>
    </div>`;

  wireUninstall(root);
}

function wireUninstall(root) {
  const search = root.querySelector('#app-search');
  if (search) {
    search.addEventListener('input', (e) => {
      uninstall.query = e.target.value;
      paintUninstall(root);
      const next = root.querySelector('#app-search');
      next.focus();
      next.setSelectionRange(next.value.length, next.value.length);
    });
  }

  root.querySelectorAll('[data-uninstall]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const app = uninstall.items.find((a) => a.id === btn.dataset.uninstall);
      if (!app) return;
      const ok = await dialog({
        title: `Uninstall ${app.name}?`,
        body: `<p>${esc(app.publisher || 'The vendor')}'s uninstaller will open in its own window. Follow its prompts to finish.</p>`,
        confirmText: 'Launch uninstaller',
      });
      if (!ok) return;
      setBusy(btn, true, 'Starting…');
      try {
        const res = await api('/uninstall', { method: 'POST', body: { id: app.id, confirm: true } });
        toast({ title: 'Uninstaller started', body: res.message, kind: 'good', timeout: 9000 });
      } catch (err) {
        toast({ title: 'Could not start uninstaller', body: err.message, kind: 'bad' });
      } finally {
        setBusy(btn, false);
      }
    });
  });
}

export const uninstallerView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Reading installed programs…')}</div></section>`;
    try {
      const res = await api('/uninstall');
      uninstall.items = res.items;
      paintUninstall(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};
