/* Application shell: sidebar, router, system summary. */

import { $, api, esc, fmtBytes, icon, setTheme, theme, toast } from './core.js';
import { healthView } from './views/health.js';
import { cleanView, browserView, cloudView } from './views/clean.js';
import { diskView, duplicateView } from './views/storage.js';
import { startupView, performanceView, uninstallerView } from './views/manage.js';
import { softwareView, driverView } from './views/updates.js';
import { statsView, settingsView } from './views/system.js';

const VIEWS = {
  health: healthView,
  clean: cleanView,
  browsers: browserView,
  cloud: cloudView,
  disk: diskView,
  duplicates: duplicateView,
  startup: startupView,
  performance: performanceView,
  software: softwareView,
  drivers: driverView,
  uninstaller: uninstallerView,
  stats: statsView,
  settings: settingsView,
};

const NAV = [
  {
    label: 'Overview',
    items: [
      { id: 'health', icon: 'health' },
      { id: 'stats', icon: 'stats' },
    ],
  },
  {
    label: 'Cleaning',
    items: [
      { id: 'clean', icon: 'clean' },
      { id: 'browsers', icon: 'globe' },
      { id: 'cloud', icon: 'cloud' },
      { id: 'disk', icon: 'disk' },
      { id: 'duplicates', icon: 'copy' },
    ],
  },
  {
    label: 'Performance',
    items: [
      { id: 'startup', icon: 'rocket' },
      { id: 'performance', icon: 'gauge' },
    ],
  },
  {
    label: 'Software',
    items: [
      { id: 'software', icon: 'box' },
      { id: 'drivers', icon: 'chip' },
      { id: 'uninstaller', icon: 'trash' },
    ],
  },
];

const TITLES = {
  health: ['Health Check', 'Scan this PC and fix what holds it back.'],
  clean: ['Custom Clean', 'Choose exactly which traces and leftovers to remove.'],
  browsers: ['Browser Cleaner', 'Clear caches and privacy traces for every installed browser.'],
  cloud: ['Cloud Drive Cleaner', 'Find clutter in your synced cloud folders.'],
  disk: ['Disk Analyzer', 'See what is actually using your space.'],
  duplicates: ['Duplicate Finder', 'Locate identical files wasting storage.'],
  startup: ['Startup Manager', 'Decide what is allowed to launch when you sign in.'],
  performance: ['Performance Optimizer', 'Inspect running processes and reclaim resources.'],
  software: ['Software Updater', 'Keep installed applications current.'],
  drivers: ['Driver Updater', 'Review device driver versions and ages.'],
  uninstaller: ['Uninstaller', 'Remove programs you no longer need.'],
  stats: ['Statistics', 'Everything OpenClean has cleaned on this PC.'],
  settings: ['Settings', 'Preferences, exclusions and safety controls.'],
};

let current = null;

function buildNav() {
  const nav = $('#nav');
  nav.innerHTML = NAV.map(
    (group) => `
      <div class="nav-group">${esc(group.label)}</div>
      ${group.items
        .map(
          (item) => `
        <button class="nav-item" data-view="${item.id}" type="button">
          ${icon(item.icon)}
          <span>${esc(VIEWS[item.id].title || TITLES[item.id][0])}</span>
        </button>`
        )
        .join('')}`
  ).join('') + `
    <div class="nav-group">System</div>
    <button class="nav-item" data-view="settings" type="button">${icon('settings')}<span>Settings</span></button>`;
}

async function loadSystemCard() {
  try {
    const info = await api('/system');
    $('#pc-name').textContent = info.hostname || 'This PC';
    const dot = $('#admin-dot');
    dot.classList.toggle('admin', !!info.admin);
    dot.title = info.admin ? 'Running as administrator' : 'Standard user rights';

    const drive = (info.drives || []).find((d) => d.letter === 'C:') || (info.drives || [])[0];
    if (drive && drive.total) {
      const usedPct = ((drive.total - drive.free) / drive.total) * 100;
      $('#disk-bar').style.width = `${Math.min(100, Math.max(2, usedPct)).toFixed(1)}%`;
      $('#disk-bar').style.background =
        usedPct > 92
          ? 'linear-gradient(90deg,#e04a57,#ff5f6d)'
          : usedPct > 80
          ? 'linear-gradient(90deg,#e09a2a,#f5b544)'
          : 'linear-gradient(90deg,var(--accent),var(--accent-2))';
      $('#pc-disk').textContent = `${fmtBytes(drive.free)} free on ${drive.letter}`;
    } else {
      $('#pc-disk').textContent = 'Drive information unavailable';
    }
    return info;
  } catch {
    $('#pc-disk').textContent = 'Offline';
    return null;
  }
}

export async function refreshShell() {
  await loadSystemCard();
}

function setActiveView(id) {
  document.querySelectorAll('.nav-item').forEach((el) => {
    el.classList.toggle('active', el.dataset.view === id);
  });
}

async function navigate(id, { replace = false } = {}) {
  const view = VIEWS[id] || VIEWS.health;
  const key = VIEWS[id] ? id : 'health';

  if (!replace && window.location.hash !== `#/${key}`) {
    history.pushState(null, '', `#/${key}`);
  }

  current = key;
  setActiveView(key);

  const [title, subtitle] = TITLES[key];
  $('#page-title').textContent = view.title || title;
  $('#page-subtitle').textContent = view.subtitle || subtitle;
  document.title = `OpenClean — ${view.title || title}`;

  const actions = $('#topbar-actions');
  actions.innerHTML = typeof view.actions === 'string' ? view.actions : '';
  if (typeof view.onActions === 'function') view.onActions(actions);

  const root = $('#view');
  root.innerHTML = `<div class="loading"><span class="spinner"></span> Loading…</div>`;
  try {
    await view.render(root);
  } catch (err) {
    root.innerHTML = `<div class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div><strong>This screen could not load.</strong><br>${esc(err.message)}</div></div></div></div>`;
  }
  window.scrollTo({ top: 0, behavior: 'instant' in window ? 'instant' : 'auto' });
}

function wireGlobal() {
  $('#nav').addEventListener('click', (e) => {
    const item = e.target.closest('[data-view]');
    if (item) navigate(item.dataset.view);
  });

  window.addEventListener('popstate', () => {
    const id = (window.location.hash || '#/health').replace('#/', '');
    navigate(id, { replace: true });
  });

  // Theme: instant local switch, then persisted server-side.
  const initial = localStorage.getItem('openclean-theme');
  if (initial) setTheme(initial);

  $('#theme-toggle').addEventListener('click', async () => {
    const next = theme() === 'dark' ? 'light' : 'dark';
    setTheme(next);
    try {
      await api('/settings', { method: 'POST', body: { patch: { theme: next } } });
    } catch {
      /* preference still applies for this session */
    }
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      const root = document.getElementById('modal-root');
      if (root) root.innerHTML = '';
    }
  });
}

async function boot() {
  buildNav();
  wireGlobal();

  try {
    const settings = await api('/settings');
    if (!localStorage.getItem('openclean-theme') && settings.theme) setTheme(settings.theme);
    $('#brand-version').textContent = 'v1.0.0';
  } catch {
    /* defaults are fine */
  }

  loadSystemCard();

  const initial = (window.location.hash || '#/health').replace('#/', '') || 'health';
  navigate(initial, { replace: true });

  // Keep the drive readout fresh without polling hard.
  setInterval(() => {
    if (document.visibilityState === 'visible') loadSystemCard();
  }, 120000);
}

boot().catch((err) => {
  toast({ title: 'OpenClean failed to start', body: err.message, kind: 'bad', timeout: 12000 });
});
