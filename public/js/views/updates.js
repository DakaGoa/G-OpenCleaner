/* Software Updater and Driver Updater. */

import {
  $, api, dialog, emptyBlock, esc, fmtBytes, fmtCount, icon, loadingBlock, pill, setBusy, shortPath, toast,
} from '../core.js';

/* -------------------------------------------------------- Software Updater */

const software = { updates: null, supported: null, busy: false, checking: false, installed: 0 };

function paintSoftware(root, { preserveScroll = true } = {}) {
  const scrollTop = preserveScroll ? root.querySelector('.table-scroll')?.scrollTop || 0 : 0;
  const u = software.updates;

  root.innerHTML = `
    <div class="stack">
      <div class="grid grid-3">
        <div class="stat"><span class="stat-label">Installed programs</span><strong class="stat-value">${fmtCount(software.installed)}</strong><span class="stat-hint">registered with Windows</span></div>
        <div class="stat"><span class="stat-label">Updates available</span><strong class="stat-value">${u ? u.items.length : '—'}</strong><span class="stat-hint">${u ? 'via Windows Package Manager' : 'not checked yet'}</span></div>
        <div class="stat"><span class="stat-label">Package manager</span><strong class="stat-value">${software.supported === null ? '—' : software.supported ? 'Ready' : 'Missing'}</strong><span class="stat-hint">winget</span></div>
      </div>

      <section class="card">
        <div class="card-head">
          <div><h2>Application updates</h2><p>Updates are installed with winget, the package manager built into Windows.</p></div>
          <div class="card-actions">
            <button class="btn btn-sm ${u && u.items.length ? 'btn-ghost' : 'btn-primary'}" data-act="check" type="button" ${software.checking ? 'disabled' : ''}>
              ${software.checking ? '<span class="spinner"></span> Checking…' : `${icon('refresh')} Check for updates`}
            </button>
          </div>
        </div>
        <div class="card-body ${u && u.items.length ? 'flush' : ''}">
          ${software.supported === false
            ? `<div class="inline-note warn">${icon('alert')}<div><strong>Windows Package Manager (winget) is not available.</strong><br>Install <em>App Installer</em> from the Microsoft Store to let OpenClean check application updates.</div></div>`
            : !u
            ? emptyBlock('Not checked yet', 'Press “Check for updates” to ask winget what is out of date.', 'box')
            : u.items.length
            ? `<div class="table-scroll"><table class="table">
                 <thead><tr><th>Package</th><th>Identifier</th><th>Installed</th><th>Available</th><th></th></tr></thead>
                 <tbody>
                   ${u.items.map((item) => `
                     <tr>
                       <td class="truncate" style="max-width:280px"><strong style="font-size:13px">${esc(item.name)}</strong></td>
                       <td class="truncate mono tiny muted" style="max-width:200px">${esc(item.id)}</td>
                       <td class="muted nowrap">${esc(item.version || '—')}</td>
                       <td class="nowrap">${pill('accent', item.available || 'newer')}</td>
                       <td style="text-align:right"><button class="btn btn-sm" data-upgrade="${esc(item.id)}" data-name="${esc(item.name)}" type="button">Update</button></td>
                     </tr>`).join('')}
                 </tbody>
               </table></div>`
            : emptyBlock('Everything is current', 'No package reported an available update.', 'check')}
        </div>
      </section>

      <div class="foot-note">Updates run silently and can take a few minutes each. Some packages need administrator rights and will report that rather than fail silently.</div>
    </div>`;

  wireSoftware(root);
  if (preserveScroll && scrollTop) {
    const el = root.querySelector('.table-scroll');
    if (el) el.scrollTop = scrollTop;
  }
}

function wireSoftware(root) {
  const check = root.querySelector('[data-act="check"]');
  if (check) check.addEventListener('click', async () => {
    software.checking = true;
    paintSoftware(root);
    try {
      const res = await api('/software/updates');
      software.supported = res.supported;
      software.updates = res.supported ? { items: res.items } : null;
      if (!res.supported) toast({ title: 'winget not found', body: 'Install App Installer to check updates.', kind: 'warn' });
      else if (!res.items.length) toast({ title: 'Everything is up to date', kind: 'good' });
    } catch (err) {
      toast({ title: 'Update check failed', body: err.message, kind: 'bad' });
    } finally {
      software.checking = false;
      paintSoftware(root, { preserveScroll: false });
    }
  });

  root.querySelectorAll('[data-upgrade]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const ok = await dialog({
        title: `Update ${btn.dataset.name}?`,
        body: `<p>winget will download and install the newest version. The application may close while it updates.</p>`,
        confirmText: 'Update now',
      });
      if (!ok) return;
      setBusy(btn, true, 'Updating…');
      try {
        const res = await api('/software/upgrade', { method: 'POST', body: { id: btn.dataset.upgrade, confirm: true } });
        toast({ title: `${btn.dataset.name} updated`, body: res.output ? res.output.split('\n').slice(-1)[0] : '', kind: 'good', timeout: 8000 });
        software.updates.items = software.updates.items.filter((i) => i.id !== btn.dataset.upgrade);
        paintSoftware(root);
      } catch (err) {
        toast({ title: 'Update failed', body: err.message, kind: 'bad', timeout: 10000 });
        setBusy(btn, false);
      }
    });
  });
}

export const softwareView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Reading installed programs…')}</div></section>`;
    try {
      const res = await api('/software');
      software.installed = res.count;
      paintSoftware(root, { preserveScroll: false });
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};

/* ---------------------------------------------------------- Driver Updater */

const driver = { items: [], total: 0, old: 0, onlyOld: false, query: '', loaded: false, restoreBusy: false };

function filteredDrivers() {
  const q = driver.query.trim().toLowerCase();
  return driver.items.filter((d) => {
    if (driver.onlyOld && !d.old) return false;
    if (q && !`${d.name} ${d.manufacturer} ${d.deviceClass}`.toLowerCase().includes(q)) return false;
    return true;
  });
}

function paintDrivers(root) {
  const list = filteredDrivers();

  root.innerHTML = `
    <div class="stack">
      <div class="grid grid-3">
        <div class="stat"><span class="stat-label">Devices</span><strong class="stat-value">${fmtCount(driver.total)}</strong><span class="stat-hint">with a signed driver</span></div>
        <div class="stat"><span class="stat-label">Older than 3 years</span><strong class="stat-value">${fmtCount(driver.old)}</strong><span class="stat-hint">${driver.old > 8 ? 'worth reviewing' : 'normal for stable hardware'}</span></div>
        <div class="stat"><span class="stat-label">Safety</span><strong class="stat-value" style="font-size:15px">Restore point</strong><span class="stat-hint">create one before driver work</span></div>
      </div>

      <div class="inline-note">${icon('info')}<div>Driver versions come straight from Windows device management. OpenClean does not download drivers — install them from your PC or hardware maker, then re-check here. This avoids shipping a third-party driver database that goes stale.</div></div>

      <section class="card">
        <div class="list-toolbar">
          <div class="search">
            ${icon('search')}
            <input type="search" id="driver-search" placeholder="Filter devices" value="${esc(driver.query)}" />
          </div>
          <label class="toggle" style="margin-left:6px">
            <input type="checkbox" id="driver-old" ${driver.onlyOld ? 'checked' : ''}>
            <span class="track"></span>
            <span class="tiny dim">Only old drivers</span>
          </label>
          <div class="spacer"></div>
          <button class="btn btn-sm btn-ghost" data-act="restore" type="button" ${driver.restoreBusy ? 'disabled' : ''}>
            ${driver.restoreBusy ? '<span class="spinner"></span> Creating…' : `${icon('shield')} Create restore point`}
          </button>
        </div>
        <div class="card-body flush">
          ${list.length ? `
            <div class="table-scroll" style="max-height:60vh">
              <table class="table">
                <thead><tr><th>Device</th><th>Class</th><th>Vendor</th><th>Version</th><th>Driver date</th></tr></thead>
                <tbody>
                  ${list.map((d) => `
                    <tr>
                      <td class="truncate" style="max-width:300px"><strong style="font-size:13px">${esc(d.name)}</strong></td>
                      <td class="muted nowrap">${esc(d.deviceClass || '—')}</td>
                      <td class="truncate muted" style="max-width:170px">${esc(d.manufacturer || '—')}</td>
                      <td class="mono tiny muted nowrap">${esc(d.version || '—')}</td>
                      <td class="nowrap">${d.old ? pill('warn', d.date || 'old') : `<span class="muted">${esc(d.date || '—')}</span>`}</td>
                    </tr>`).join('')}
                </tbody>
              </table>
            </div>` : emptyBlock('No devices match', 'Clear the filter to see all devices.', 'chip')}
        </div>
      </section>
    </div>`;

  wireDrivers(root);
}

function wireDrivers(root) {
  const search = root.querySelector('#driver-search');
  search.addEventListener('input', (e) => {
    driver.query = e.target.value;
    paintDrivers(root);
    const next = root.querySelector('#driver-search');
    next.focus();
    next.setSelectionRange(next.value.length, next.value.length);
  });

  root.querySelector('#driver-old').addEventListener('change', (e) => {
    driver.onlyOld = e.target.checked;
    paintDrivers(root);
  });

  root.querySelector('[data-act="restore"]').addEventListener('click', async (e) => {
    const btn = e.currentTarget;
    const ok = await dialog({
      title: 'Create a system restore point?',
      body: `<p>Windows will snapshot the current system state so you can roll back after driver or software changes. This needs administrator rights and can take a minute.</p>`,
      confirmText: 'Create restore point',
    });
    if (!ok) return;
    driver.restoreBusy = true;
    setBusy(btn, true, 'Creating…');
    try {
      await api('/restorepoint', { method: 'POST', body: { confirm: true, label: 'OpenClean' } });
      toast({ title: 'Restore point created', kind: 'good' });
    } catch (err) {
      toast({ title: 'Could not create restore point', body: err.message, kind: 'bad', timeout: 9000 });
    } finally {
      driver.restoreBusy = false;
      paintDrivers($('#view'));
    }
  });
}

export const driverView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Enumerating device drivers…')}</div></section>`;
    try {
      const res = await api('/drivers');
      driver.items = res.items;
      driver.total = res.total;
      driver.old = res.old;
      paintDrivers(root);
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};
