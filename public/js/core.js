/* Tiny UI toolkit: fetch wrapper, formatting, toasts, dialogs, icons. */

const token = document.querySelector('meta[name="x-cc-token"]').content;

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

export async function api(path, { method = 'GET', body } = {}) {
  const res = await fetch(`/api${path}`, {
    method,
    headers: {
      'x-cc-token': token,
      ...(body ? { 'content-type': 'application/json' } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });

  let payload = null;
  try {
    payload = await res.json();
  } catch {
    payload = null;
  }

  if (!res.ok) {
    const err = new Error((payload && payload.error) || `Request failed (${res.status})`);
    err.status = res.status;
    throw err;
  }
  return payload;
}

/* ------------------------------------------------------------ formatting */

export function fmtBytes(bytes, forceDecimals = false) {
  const n = Number(bytes) || 0;
  if (n < 1) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  const value = n / 1024 ** i;
  const decimals = forceDecimals || (value < 10 && i > 1) ? 1 : 0;
  return `${value.toFixed(i === 0 ? 0 : decimals)} ${units[i]}`;
}

export function fmtCount(n) {
  return (Number(n) || 0).toLocaleString();
}

export function fmtDate(iso) {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return String(iso);
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: '2-digit' });
}

export function timeAgo(iso) {
  if (!iso) return 'never';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return 'never';
  const sec = Math.max(0, (Date.now() - d.getTime()) / 1000);
  if (sec < 60) return 'just now';
  const mins = Math.floor(sec / 60);
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} d ago`;
  return fmtDate(iso);
}

export function fmtDuration(seconds) {
  const s = Math.floor(Number(seconds) || 0);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d) return `${d}d ${h}h`;
  if (h) return `${h}h ${m}m`;
  return `${m}m`;
}

export function esc(value) {
  return String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[c]);
}

export function shortPath(p, max = 62) {
  const value = String(p || '');
  if (value.length <= max) return value;
  const parts = value.split('\\');
  if (parts.length <= 2) return value.slice(0, max - 1) + '…';
  return `${parts[0]}\\…\\${parts.slice(-2).join('\\')}`.slice(0, max + 8);
}

/* ----------------------------------------------------------------- icons */

const PATHS = {
  health: '<path d="M3 12h3.5l2.5-6 4 12 2.5-6H21" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  clean: '<path d="M4 20h6M9.5 20l1.2-6.5a5 5 0 011.6-2.9l3.4-3.4a2 2 0 013 3l-3.4 3.4a5 5 0 01-2.9 1.6L11 16.5" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round" stroke-linejoin="round"/><path d="M15 4l2 2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>',
  globe: '<circle cx="12" cy="12" r="8.5" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M3.5 12h17M12 3.5c2.5 2.4 2.5 14.6 0 17M12 3.5c-2.5 2.4-2.5 14.6 0 17" stroke="currentColor" stroke-width="1.6" fill="none"/>',
  disk: '<circle cx="12" cy="12" r="8.5" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M12 12V3.5M12 12l6 6" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>',
  copy: '<rect x="8.5" y="8.5" width="11" height="11" rx="2.5" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M15.5 5.5H6.5a2 2 0 00-2 2v9" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" fill="none"/>',
  gauge: '<path d="M4 16a8.5 8.5 0 1116 0" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round"/><path d="M12 16l3.5-4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="12" cy="16.4" r="1.3" fill="currentColor"/>',
  rocket: '<path d="M13 4c3 0 7 4 7 7l-4.5 1.5L10 8.5 11.5 4H13z" stroke="currentColor" stroke-width="1.6" fill="none" stroke-linejoin="round"/><path d="M10 8.5L6.5 10 5 14.5l3.5-1M13 12l-1.5 3.5L15 16l1-3.5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linejoin="round"/><path d="M7 17c-1 1-1.5 3-1.5 3s2-.5 3-1.5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round"/>',
  box: '<path d="M12 3.5l7.5 4v9L12 20.5 4.5 16.5v-9L12 3.5z" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linejoin="round"/><path d="M4.5 7.5L12 11.5l7.5-4M12 11.5v9" stroke="currentColor" stroke-width="1.5" fill="none"/>',
  chip: '<rect x="7" y="7" width="10" height="10" rx="2" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M10 3.5v3M14 3.5v3M10 17.5v3M14 17.5v3M3.5 10h3M3.5 14h3M17.5 10h3M17.5 14h3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>',
  trash: '<path d="M5 7h14M10 7V5.5A1.5 1.5 0 0111.5 4h1A1.5 1.5 0 0114 5.5V7M7 7l1 12.5h8L17 7" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  cloud: '<path d="M7.5 18.5h9a3.5 3.5 0 00.5-6.96 5.5 5.5 0 00-10.7-1.2A3.9 3.9 0 007.5 18.5z" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linejoin="round"/>',
  stats: '<path d="M4 19V11M9.5 19V5M15 19v-6M20.5 19V8" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>',
  settings: '<circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M12 3.5v2M12 18.5v2M4.9 7.5l1.7 1M17.4 15.5l1.7 1M4.9 16.5l1.7-1M17.4 8.5l1.7-1" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>',
  search: '<circle cx="11" cy="11" r="6.5" stroke="currentColor" stroke-width="1.8" fill="none"/><path d="M16 16l4 4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>',
  refresh: '<path d="M20 12a8 8 0 11-2.6-5.9M20 4v4h-4" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  alert: '<path d="M12 4l9 16H3L12 4z" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linejoin="round"/><path d="M12 10v4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="12" cy="17.4" r="1" fill="currentColor"/>',
  info: '<circle cx="12" cy="12" r="8.5" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M12 11v6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="12" cy="8.2" r="1" fill="currentColor"/>',
  x: '<path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/>',
  folder: '<path d="M3.5 7.5A2 2 0 015.5 5.5h4l2 2.5h7a2 2 0 012 2v7a2 2 0 01-2 2h-13a2 2 0 01-2-2v-9z" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linejoin="round"/>',
  play: '<path d="M8 5.5l10 6.5-10 6.5v-13z" fill="currentColor"/>',
  shield: '<path d="M12 3.5l7 2.5v5.5c0 4-3 7-7 8.5-4-1.5-7-4.5-7-8.5V6l7-2.5z" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linejoin="round"/><path d="M9 12l2 2 4-4" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  lock: '<rect x="5" y="10.5" width="14" height="9" rx="2.2" stroke="currentColor" stroke-width="1.7" fill="none"/><path d="M8.5 10.5V8a3.5 3.5 0 017 0v2.5" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round"/>',
  down: '<path d="M12 4v11M7.5 11l4.5 4.5L16.5 11M5 19.5h14" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  power: '<path d="M12 3.5v8" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/><path d="M6.5 7a8 8 0 1011 0" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round"/>',
};

export function icon(name, cls = '') {
  const path = PATHS[name] || PATHS.info;
  return `<svg viewBox="0 0 24 24" class="${cls}" aria-hidden="true" fill="none">${path}</svg>`;
}

/* ---------------------------------------------------------------- toasts */

export function toast({ title, body = '', kind = 'info', timeout = 4200 } = {}) {
  const host = document.getElementById('toasts');
  const node = document.createElement('div');
  node.className = `toast ${kind}`;
  node.innerHTML = `<span class="t-dot"></span><div class="t-body"><strong>${esc(title)}</strong>${body ? `<span>${esc(body)}</span>` : ''}</div>`;
  host.appendChild(node);
  const close = () => {
    node.classList.add('out');
    setTimeout(() => node.remove(), 220);
  };
  setTimeout(close, timeout);
  node.addEventListener('click', close);
  return node;
}

/* ---------------------------------------------------------------- dialog */

export function dialog({ title, body, confirmText = 'Confirm', cancelText = 'Cancel', danger = false }) {
  return new Promise((resolve) => {
    const root = document.getElementById('modal-root');
    const wrap = document.createElement('div');
    wrap.className = 'modal-backdrop';
    wrap.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="dlg-title">
        <div class="modal-head">
          <h3 id="dlg-title">${esc(title)}</h3>
        </div>
        <div class="modal-body">${body}</div>
        <div class="modal-foot">
          <button class="btn btn-ghost" data-act="cancel" type="button">${esc(cancelText)}</button>
          <button class="btn ${danger ? 'btn-danger' : 'btn-primary'}" data-act="ok" type="button">${esc(confirmText)}</button>
        </div>
      </div>`;
    root.innerHTML = '';
    root.appendChild(wrap);

    const done = (value) => {
      root.innerHTML = '';
      document.removeEventListener('keydown', onKey);
      resolve(value);
    };
    const onKey = (e) => {
      if (e.key === 'Escape') done(false);
      if (e.key === 'Enter') done(true);
    };

    wrap.addEventListener('click', (e) => {
      if (e.target === wrap) done(false);
      const act = e.target.closest('[data-act]');
      if (!act) return;
      done(act.dataset.act === 'ok');
    });
    document.addEventListener('keydown', onKey);
    wrap.querySelector('[data-act="ok"]').focus();
  });
}

/* ----------------------------------------------------------------- utils */

export function setBusy(button, busy, label = 'Working…') {
  if (!button) return;
  if (busy) {
    button.dataset.label = button.innerHTML;
    button.innerHTML = `<span class="spinner"></span> ${esc(label)}`;
    button.disabled = true;
  } else {
    if (button.dataset.label) button.innerHTML = button.dataset.label;
    button.disabled = false;
  }
}

export function loadingBlock(text = 'Working…') {
  return `<div class="loading"><span class="spinner"></span> ${esc(text)}</div>`;
}

export function emptyBlock(title, body = '', iconName = 'info') {
  return `<div class="empty">${icon(iconName)}<strong>${esc(title)}</strong>${body ? `<small>${esc(body)}</small>` : ''}</div>`;
}

export function pill(status, text) {
  const map = { good: 'pill-good', warn: 'pill-warn', bad: 'pill-bad', neutral: 'pill-neutral', accent: 'pill-accent' };
  return `<span class="pill ${map[status] || 'pill-neutral'}">${esc(text)}</span>`;
}

export function ringClass(score) {
  if (score >= 80) return 'good';
  if (score >= 55) return 'warn';
  return 'bad';
}

export function theme() {
  return document.documentElement.dataset.theme || 'dark';
}

export function setTheme(value) {
  document.documentElement.dataset.theme = value;
  try { localStorage.setItem('openclean-theme', value); } catch { /* ignore */ }
}
