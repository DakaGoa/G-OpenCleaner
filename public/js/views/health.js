/* Health Check: one score, six checks, one fix button. */

import { $, api, dialog, esc, fmtBytes, fmtCount, icon, loadingBlock, pill, ringClass, setBusy, timeAgo, toast } from '../core.js';

const state = { data: null, scanning: false, fixing: false, error: null };

function headline(score) {
  if (score >= 90) return 'This PC is in great shape';
  if (score >= 80) return 'Nearly nothing to fix';
  if (score >= 55) return 'A few things need attention';
  return 'This PC needs a clean-up';
}

function subline(d) {
  const junk = d.summary.junkBytes;
  if (junk > 1024 ** 3) return `${fmtBytes(junk)} of junk is sitting on your drive. Every item below is a cache, log, dump or browser trace that Windows and your apps rebuild on demand — no documents, photos or programs.`;
  if (junk > 0) return `${fmtBytes(junk)} of temporary data can be cleared. These are re-creatable files only, and anything in use is skipped automatically.`;
  return 'No junk worth removing, no privacy traces, and your drives have room to breathe.';
}

function checkCard(check) {
  const statusPill = { good: ['good', 'Healthy'], warn: ['warn', 'Attention'], bad: ['bad', 'Action needed'] }[check.status];
  const action = check.fixRuleIds && check.fixRuleIds.length
    ? `<button class="btn btn-sm" data-fix-check="${esc(check.id)}" type="button">Clean now</button>`
    : check.link
    ? `<button class="btn btn-sm btn-ghost" data-goto="${esc(check.link)}" type="button">Open</button>`
    : '';

  return `
    <article class="check-card">
      <header>
        <h3>${esc(check.title)}</h3>
        ${pill(statusPill[0], statusPill[1])}
      </header>
      <div class="metric">${esc(check.valueText)}</div>
      <p>${esc(check.detail)}</p>
      ${action ? `<div>${action}</div>` : ''}
    </article>`;
}

function paint() {
  const root = $('#view');
  const d = state.data;

  if (state.scanning && !d) {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Scanning temporary files, browsers and start-up items…')}</div></section>`;
    return;
  }

  if (state.error && !d) {
    root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div><strong>Scan failed</strong><br>${esc(state.error)}</div></div><div class="row-flex" style="margin-top:14px"><button class="btn btn-primary" data-act="scan">${icon('refresh')} Try again</button></div></div></section>`;
    wire(root);
    return;
  }

  if (!d) return;

  const cls = ringClass(d.score);
  const fixable = d.checks.reduce((n, c) => n + ((c.fixRuleIds && c.fixRuleIds.length) ? c.fixRuleIds.length : 0), 0);

  root.innerHTML = `
    <div class="stack">
      <section class="card">
        <div class="hero">
          <div class="ring ${cls}" style="--pct:${d.score}">
            <div class="ring-inner"><strong>${d.score}</strong><span>Health</span></div>
          </div>
          <div class="hero-copy">
            <h2>${esc(headline(d.score))}</h2>
            <p>${esc(subline(d))}</p>
            <div class="hero-metrics">
              <div class="hero-metric"><strong>${fmtBytes(d.summary.junkBytes)}</strong><span>Junk found</span></div>
              <div class="hero-metric"><strong>${fmtCount(d.summary.privacyItems)}</strong><span>Traces</span></div>
              <div class="hero-metric"><strong>${d.summary.startupItems}</strong><span>Startup apps</span></div>
              <div class="hero-metric"><strong>${fmtBytes(d.summary.freeBytes)}</strong><span>Free space</span></div>
            </div>
            <div class="row-flex" style="margin-top:18px">
              <button class="btn btn-primary" data-act="fix" ${fixable ? '' : 'disabled'}>
                ${icon('check')} Fix ${fixable ? `${fixable} issue${fixable === 1 ? '' : 's'}` : 'everything'}
              </button>
              <button class="btn btn-ghost" data-act="scan">${icon('refresh')} Scan again</button>
              <span class="tiny muted">Scanned ${esc(timeAgo(d.checkedAt))}</span>
            </div>
          </div>
        </div>
      </section>

      <section>
        <div class="check-grid">
          ${d.checks.map(checkCard).join('')}
        </div>
      </section>

      <div class="foot-note">
        Health Check reads file sizes and system counters only. Nothing is deleted until you press Fix,
        and locked or in-use files are always skipped.
      </div>
    </div>`;

  wire(root);
}

function wire(root) {
  root.querySelectorAll('[data-act="scan"]').forEach((btn) =>
    btn.addEventListener('click', () => scan(btn))
  );

  const fixBtn = root.querySelector('[data-act="fix"]');
  if (fixBtn) fixBtn.addEventListener('click', () => fixAll(fixBtn));

  root.querySelectorAll('[data-fix-check]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const check = state.data.checks.find((c) => c.id === btn.dataset.fixCheck);
      if (check) cleanRules(check.fixRuleIds, check.title, btn);
    });
  });

  root.querySelectorAll('[data-goto]').forEach((btn) => {
    btn.addEventListener('click', () => {
      window.location.hash = `#/${btn.dataset.goto}`;
    });
  });
}

async function fixAll(button) {
  if (state.fixing) return;
  const plan = state.data.fixPlan;
  const ids = plan
    ? plan.rules.map((r) => r.id)
    : [...new Set(state.data.checks.flatMap((c) => c.fixRuleIds || []))];
  if (!ids.length) return;

  const list = (plan?.rules || []).slice(0, 8);
  const more = (plan?.rules.length || 0) - list.length;
  const permanentExcluded = (plan?.excluded || []).filter((e) => e.permanent);

  const body = `
    <p>OpenClean will permanently remove <strong>${fmtBytes(plan ? plan.bytes : state.data.summary.junkBytes)}</strong> across <strong>${fmtCount(plan ? plan.count : 0)} files</strong>:</p>
    <ul class="plan-list">
      ${list.map((r) => `<li><span>${esc(r.name)}</span><span class="muted">${fmtBytes(r.bytes)} · ${fmtCount(r.count)} files</span></li>`).join('')}
      ${more > 0 ? `<li><span class="tiny muted">and ${more} more categories</span><span></span></li>` : ''}
    </ul>
    ${plan && plan.actions.length ? `<p style="margin-top:12px">It also runs: <strong>${plan.actions.map(esc).join('</strong>, <strong>')}</strong>.</p>` : ''}
    <div class="inline-note accent" style="margin-top:12px">${icon('shield')}<div>Every item above is a cache, log, memory dump or browser trace that Windows and your apps rebuild. No documents, photos, saved passwords or installed programs are touched.</div></div>
    ${permanentExcluded.length ? `<div class="inline-note warn" style="margin-top:10px">${icon('info')}<div><strong>${permanentExcluded.map((e) => esc(e.name)).join(', ')} is not emptied here.</strong> That would permanently destroy files you can still restore, so it stays a deliberate choice in Custom Clean.</div></div>` : ''}`;

  const ok = await dialog({ title: 'Fix everything found?', body, confirmText: 'Clean now' });
  if (!ok) return;

  state.fixing = true;
  setBusy(button, true, 'Cleaning…');
  try {
    const res = await api('/health/fix', { method: 'POST', body: { ruleIds: ids, confirm: true, label: 'Health Check' } });
    toast({
      title: `Freed ${fmtBytes(res.bytes)}`,
      body: `${fmtCount(res.removed)} files removed${res.skipped ? `, ${fmtCount(res.skipped)} in use` : ''}.`,
      kind: 'good',
    });
    state.data = res.health;
    state.fixing = false;
    paint();
  } catch (err) {
    toast({ title: 'Clean failed', body: err.message, kind: 'bad' });
    state.fixing = false;
    setBusy(button, false);
  }
}

async function cleanRules(ids, label, button) {
  const snapshot = state.data;
  const selected = snapshot.checks.find((c) => (c.fixRuleIds || []).some((id) => ids.includes(id)));
  const bytes = selected ? selected.value : 0;

  const actions = (selected && selected.fixActions) || [];
  const ok = await dialog({
    title: `Clean ${label.toLowerCase()}?`,
    body: `<p>${bytes ? `<strong>${fmtBytes(bytes)}</strong> will be removed.` : 'Selected items will be removed.'} Files currently in use are skipped.</p>
           ${actions.length ? `<div class="inline-note" style="margin-top:12px">${icon('info')}<div>Also runs: <strong>${actions.map(esc).join('</strong>, <strong>')}</strong>.</div></div>` : ''}`,
    confirmText: 'Clean',
  });
  if (!ok) return;

  setBusy(button, true, 'Cleaning…');
  try {
    const res = await api('/health/fix', { method: 'POST', body: { ruleIds: ids, confirm: true, label } });
    toast({ title: `Freed ${fmtBytes(res.bytes)}`, body: `${fmtCount(res.removed)} files removed.`, kind: 'good' });
    state.data = res.health;
    paint();
  } catch (err) {
    toast({ title: 'Clean failed', body: err.message, kind: 'bad' });
    setBusy(button, false);
  }
}

async function scan(button) {
  if (state.scanning) return;
  state.scanning = true;
  state.error = null;
  setBusy(button, true, 'Scanning…');
  if (!state.data) paint();

  const view = $('#view');
  if (state.data) {
    const note = document.createElement('div');
    note.className = 'inline-note accent';
    note.innerHTML = `${icon('info')}<div>Scanning… this reads file metadata and can take a few seconds.</div>`;
    view.prepend(note);
  }

  try {
    state.data = await api('/health');
  } catch (err) {
    state.error = err.message;
  } finally {
    state.scanning = false;
    paint();
    if (button && document.body.contains(button)) setBusy(button, false);
  }
}

export const healthView = {
  render(root) {
    paint();
    // First visit scans automatically; later visits reuse the last result.
    if (!state.data && !state.scanning) scan(null);
  },
};
