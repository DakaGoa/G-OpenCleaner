/* Disk Analyzer and Duplicate Finder. */

import {
  $, api, dialog, emptyBlock, esc, fmtBytes, fmtCount, icon, loadingBlock, pill, setBusy, shortPath, toast,
} from '../core.js';

/* ----------------------------------------------------------- Disk Analyzer */

const disk = { root: 'C:\\', depth: 2, data: null, busy: false, chosen: new Set() };

function breadcrumb(root) {
  const parts = root.split('\\').filter(Boolean);
  const crumbs = [];
  let acc = '';
  for (const part of parts) {
    acc = acc ? `${acc}\\${part}` : part;
    crumbs.push({ label: part, path: acc.endsWith(':') ? `${acc}\\` : acc });
  }
  return crumbs;
}

function paintDisk(root) {
  const d = disk.data;
  if (!d) {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock(`Measuring ${disk.root}…`)}</div></section>`;
    return;
  }

  const max = d.children.length ? d.children[0].size : 1;
  const crumbs = breadcrumb(d.root);
  const selectedBytes = [...disk.chosen].reduce((sum, p) => {
    const file = d.largeFiles.find((f) => f.path === p);
    return sum + (file ? file.size : 0);
  }, 0);

  root.innerHTML = `
    <div class="stack">
      <section class="card">
        <div class="list-toolbar">
          <div class="segmented">
            ${[1, 2, 3].map((n) => `<button data-depth="${n}" type="button" class="${disk.depth === n ? 'active' : ''}">${n === 1 ? 'Top level' : n === 2 ? 'Folders' : 'Deep'}</button>`).join('')}
          </div>
          <div class="search" style="flex:1;min-width:260px">
            ${icon('folder')}
            <input type="text" id="disk-path" value="${esc(disk.root)}" placeholder="C:\\Users" />
          </div>
          <button class="btn btn-sm btn-primary" data-act="disk-scan" type="button">${icon('search')} Analyze</button>
          ${crumbs.length > 1 ? `<button class="btn btn-sm btn-ghost" data-act="disk-up" type="button">Up</button>` : ''}
        </div>
        <div class="card-body">
          <div class="row-flex" style="justify-content:space-between;align-items:flex-end">
            <div>
              <div class="stat-value" style="font-size:28px">${fmtBytes(d.total)}</div>
              <small class="muted">${fmtCount(d.fileCount)} files in ${esc(shortPath(d.root, 58))}</small>
            </div>
            ${d.truncated ? pill('warn', 'partial scan') : pill('good', 'complete')}
          </div>
          <div class="tiny muted" style="margin-top:10px">
            ${crumbs.map((c, i) => `<button class="btn-link" data-crumb="${esc(c.path)}" type="button">${esc(c.label)}</button>${i < crumbs.length - 1 ? '<span class="muted"> \\ </span>' : ''}`).join('')}
          </div>
        </div>
      </section>

      <section class="card">
        <div class="card-head">
          <div><h2>Space by folder</h2><p>Click any row to drill into it.</p></div>
          <div class="card-actions"><span class="tiny muted">${d.children.length} entries</span></div>
        </div>
        <div class="card-body">
          ${d.children.length ? d.children.map((child) => `
            <div class="bar-row clickable" data-open="${esc(child.path)}">
              <span class="bar-label mono">${esc(shortPath(child.path.replace(d.root, '') || child.path, 40).replace(/^\\+/, ''))}</span>
              <div class="bar"><span style="width:${Math.max(1, (child.size / max) * 100).toFixed(2)}%"></span></div>
              <span class="bar-value">${fmtBytes(child.size)} · ${fmtCount(child.files)}</span>
            </div>`).join('') : emptyBlock('Nothing here')}
        </div>
      </section>

      <section class="card">
        <div class="card-head">
          <div><h2>Large files</h2><p>Files over 50 MB, biggest first.</p></div>
        </div>
        <div class="card-body flush">
          ${d.largeFiles.length ? `
          <div class="table-scroll">
            <table class="table">
              <thead><tr><th style="width:34px"></th><th>File</th><th class="num">Size</th></tr></thead>
              <tbody>
                ${d.largeFiles.map((file) => `
                  <tr>
                    <td><label class="check"><input type="checkbox" data-large="${esc(file.path)}" ${disk.chosen.has(file.path) ? 'checked' : ''}><span></span></label></td>
                    <td class="truncate mono" title="${esc(file.path)}">${esc(shortPath(file.path, 76))}</td>
                    <td class="num nowrap">${fmtBytes(file.size)}</td>
                  </tr>`).join('')}
              </tbody>
            </table>
          </div>
          <div class="result-summary">
            <div><div class="rs-value">${fmtBytes(selectedBytes)}</div><div class="tiny muted">${disk.chosen.size} files selected</div></div>
            <div class="spacer"></div>
            <button class="btn btn-sm btn-danger" data-act="disk-delete" type="button" ${disk.chosen.size ? '' : 'disabled'}>${icon('trash')} Delete selected</button>
          </div>` : emptyBlock('No large files found', 'This location has no files over 50 MB.', 'disk')}
        </div>
      </section>
    </div>`;

  wireDisk(root);
}

function wireDisk(root) {
  root.querySelectorAll('[data-depth]').forEach((btn) => {
    btn.addEventListener('click', () => { disk.depth = Number(btn.dataset.depth); scanDisk(disk.root, btn); });
  });

  root.querySelectorAll('[data-open]').forEach((row) => {
    row.addEventListener('click', () => scanDisk(row.dataset.open));
  });

  root.querySelectorAll('[data-crumb]').forEach((btn) => {
    btn.addEventListener('click', () => scanDisk(btn.dataset.crumb));
  });

  const up = root.querySelector('[data-act="disk-up"]');
  if (up) up.addEventListener('click', () => {
    const parent = disk.root.replace(/\\+$/, '').split('\\').slice(0, -1).join('\\');
    if (parent) scanDisk(/^[A-Za-z]:$/.test(parent) ? `${parent}\\` : parent);
  });

  const input = root.querySelector('#disk-path');
  const scanBtn = root.querySelector('[data-act="disk-scan"]');
  scanBtn.addEventListener('click', () => scanDisk(input.value.trim() || disk.root, scanBtn));
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') scanDisk(input.value.trim(), scanBtn); });

  root.querySelectorAll('[data-large]').forEach((box) => {
    box.addEventListener('change', () => {
      if (box.checked) disk.chosen.add(box.dataset.large);
      else disk.chosen.delete(box.dataset.large);
      paintDisk(root);
    });
  });

  const del = root.querySelector('[data-act="disk-delete"]');
  if (del) del.addEventListener('click', async () => {
    const files = [...disk.chosen];
    if (!files.length) return;
    const total = disk.data.largeFiles.filter((f) => files.includes(f.path)).reduce((s, f) => s + f.size, 0);
    const ok = await dialog({
      title: 'Delete these files permanently?',
      body: `<p>${files.length} files (${fmtBytes(total)}) will be deleted. They do not go to the Recycle Bin.</p>
             <div class="inline-note warn" style="margin-top:12px">${icon('alert')}<div>Check the list carefully — these are your own files, not cache.</div></div>`,
      confirmText: 'Delete permanently',
      danger: true,
    });
    if (!ok) return;

    setBusy(del, true, 'Deleting…');
    try {
      const res = await api('/files/delete', {
        method: 'POST',
        body: { paths: files, root: disk.root, confirm: true, label: 'Disk Analyzer' },
      });
      toast({ title: `Freed ${fmtBytes(res.bytes)}`, body: `${fmtCount(res.removed)} files deleted.`, kind: 'good' });
      disk.chosen.clear();
      scanDisk(disk.root);
    } catch (err) {
      toast({ title: 'Delete failed', body: err.message, kind: 'bad' });
      setBusy(del, false);
    }
  });
}

async function scanDisk(target, button) {
  if (!target) return;
  disk.root = target;
  disk.data = null;
  disk.chosen.clear();
  paintDisk($('#view'));
  if (button) setBusy(button, true, 'Measuring…');
  try {
    disk.data = await api(`/disk?path=${encodeURIComponent(target)}&depth=${disk.depth}`);
  } catch (err) {
    toast({ title: 'Could not analyze that folder', body: err.message, kind: 'bad' });
    disk.data = { root: target, total: 0, fileCount: 0, children: [], largeFiles: [], truncated: false };
  }
  paintDisk($('#view'));
}

export const diskView = {
  render: async (root) => {
    paintDisk(root);
    if (!disk.data) await scanDisk(disk.root);
  },
};

/* -------------------------------------------------------- Duplicate Finder */

const dup = { folders: [], roots: [], results: null, busy: false, chosen: new Set(), minSize: 1024 ** 2 };

function folderChips() {
  if (!dup.roots.length) return `<span class="tiny muted">Add a folder to search.</span>`;
  return dup.roots.map((folder) => `
    <span class="pill pill-neutral">
      ${esc(shortPath(folder, 40))}
      <button class="btn-link" data-remove-root="${esc(folder)}" type="button" style="color:var(--bad)">×</button>
    </span>`).join('');
}

function paintDup(root) {
  const r = dup.results;
  root.innerHTML = `
    <div class="stack">
      <section class="card">
        <div class="card-head">
          <div><h2>Where to look</h2><p>Identical files are matched by content, never by name alone.</p></div>
          <div class="card-actions">
            <select id="dup-min">
              ${[[1, '1 MB'], [10, '10 MB'], [100, '100 MB'], [0.1, '100 KB']].map(([mb, label]) =>
                `<option value="${mb}" ${String(dup.minSize / 1024 ** 2) === String(mb) ? 'selected' : ''}>Over ${label}</option>`).join('')}
            </select>
          </div>
        </div>
        <div class="card-body">
          <div class="row-flex" style="margin-bottom:12px">${folderChips()}</div>
          <div class="row-flex">
            <div class="search" style="flex:1;min-width:260px">
              ${icon('folder')}
              <input type="text" id="dup-path" placeholder="C:\\Users\\you\\Downloads" />
            </div>
            <button class="btn btn-sm btn-ghost" data-quick="downloads" type="button">Downloads</button>
            <button class="btn btn-sm btn-ghost" data-quick="pictures" type="button">Pictures</button>
            <button class="btn btn-sm btn-ghost" data-quick="documents" type="button">Documents</button>
            <button class="btn btn-sm btn-primary" data-act="dup-scan" type="button">${icon('copy')} Scan</button>
          </div>
          ${r ? `<div class="tiny muted" style="margin-top:10px">${fmtCount(r.scanned)} files scanned · ${fmtCount(r.candidates)} candidates · ${r.truncated ? 'partial scan' : 'complete'}</div>` : ''}
        </div>
      </section>

      ${r ? (r.groups.length ? `
      <section class="card">
        <div class="card-head">
          <div>
            <h2>${fmtCount(r.groupCount)} duplicate groups</h2>
            <p>${fmtBytes(r.wasted)} could be reclaimed. The oldest copy in each group is left ticked off — untick anything you want to keep.</p>
          </div>
        </div>
        <div class="card-body flush">
          <div class="table-scroll">
            ${r.groups.map((group, gi) => `
              <div class="group-head">
                <span>${fmtBytes(group.size)} × ${group.count} copies</span>
                <div class="bulk"><span class="tiny muted">wasted ${fmtBytes(group.wasted)}</span></div>
              </div>
              ${group.files.map((file) => `
                <label class="pick-row">
                  <span class="check"><input type="checkbox" data-dup-file="${esc(file.path)}" ${dup.chosen.has(file.path) ? 'checked' : ''}><span></span></span>
                  <div class="pick-main">
                    <strong class="mono" style="font-weight:500">${esc(shortPath(file.path, 78))}</strong>
                  </div>
                  <div class="row-end nowrap">${fmtBytes(file.size)}</div>
                </label>`).join('')}
            `).join('')}
          </div>
          <div class="result-summary">
            <div><div class="rs-value">${fmtBytes(selectedDupBytes())}</div><div class="tiny muted">${dup.chosen.size} files selected</div></div>
            <div class="spacer"></div>
            <button class="btn btn-sm btn-danger" data-act="dup-delete" type="button" ${dup.chosen.size ? '' : 'disabled'}>${icon('trash')} Delete selected</button>
          </div>
        </div>
      </section>` : `<section class="card"><div class="card-body">${emptyBlock('No duplicates found', 'Try a lower size threshold or add more folders.', 'copy')}</div></section>`) : ''}
    </div>`;

  wireDup(root);
}

function selectedDupBytes() {
  if (!dup.results) return 0;
  let total = 0;
  for (const group of dup.results.groups) {
    for (const file of group.files) {
      if (dup.chosen.has(file.path)) total += file.size;
    }
  }
  return total;
}

function wireDup(root) {
  const scanBtn = root.querySelector('[data-act="dup-scan"]');
  const input = root.querySelector('#dup-path');

  const addRoot = (folder) => {
    if (!folder) return;
    if (!dup.roots.includes(folder)) dup.roots.push(folder);
    paintDup(root);
  };

  root.querySelector('#dup-min').addEventListener('change', (e) => {
    dup.minSize = Math.round(Number(e.target.value) * 1024 ** 2);
  });

  const home = dup.folders?.home || '';
  const quick = { downloads: 'Downloads', pictures: 'Pictures', documents: 'Documents' };
  root.querySelectorAll('[data-quick]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const folder = home ? `${home}\\${quick[btn.dataset.quick]}` : '';
      addRoot(folder);
    });
  });

  scanBtn.addEventListener('click', () => {
    const value = input.value.trim();
    if (value) {
      if (!dup.roots.includes(value)) dup.roots.push(value);
      input.value = '';
    }
    runDupScan(scanBtn);
  });

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const value = input.value.trim();
      if (value) dup.roots.push(value);
      input.value = '';
      runDupScan(scanBtn);
    }
  });

  root.querySelectorAll('[data-remove-root]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.preventDefault();
      dup.roots = dup.roots.filter((r) => r !== btn.dataset.removeRoot);
      paintDup(root);
    });
  });

  root.querySelectorAll('[data-dup-file]').forEach((box) => {
    box.addEventListener('change', () => {
      if (box.checked) dup.chosen.add(box.dataset.dupFile);
      else dup.chosen.delete(box.dataset.dupFile);
      paintDup(root);
    });
  });

  const del = root.querySelector('[data-act="dup-delete"]');
  if (del) del.addEventListener('click', async () => {
    const files = [...dup.chosen];
    const ok = await dialog({
      title: 'Delete duplicate copies?',
      body: `<p>${files.length} files (${fmtBytes(selectedDupBytes())}) will be permanently deleted. One copy of each file stays behind.</p>`,
      confirmText: 'Delete copies',
      danger: true,
    });
    if (!ok) return;
    setBusy(del, true, 'Deleting…');
    try {
      const res = await api('/files/delete', {
        method: 'POST',
        body: { paths: files, root: dup.roots, confirm: true, label: 'Duplicate Finder' },
      });
      toast({ title: `Reclaimed ${fmtBytes(res.bytes)}`, body: `${fmtCount(res.removed)} duplicate files deleted.`, kind: 'good' });
      dup.chosen.clear();
      runDupScan(null);
    } catch (err) {
      toast({ title: 'Delete failed', body: err.message, kind: 'bad' });
      setBusy(del, false);
    }
  });
}

async function runDupScan(button) {
  if (!dup.roots.length) {
    toast({ title: 'Add a folder first', body: 'Duplicate Finder needs at least one folder to search.', kind: 'warn' });
    return;
  }
  if (button) setBusy(button, true, 'Scanning…');
  const root = $('#view');
  const note = document.createElement('div');
  try {
    dup.results = await api('/duplicates', { method: 'POST', body: { roots: dup.roots, minSize: dup.minSize } });
    dup.chosen = new Set();
    for (const group of dup.results.groups) {
      const oldestFirst = [...group.files].sort((a, b) => a.mtime - b.mtime);
      oldestFirst.slice(1).forEach((file) => dup.chosen.add(file.path));
    }
    paintDup(root);
  } catch (err) {
    toast({ title: 'Scan failed', body: err.message, kind: 'bad' });
    if (button) setBusy(button, false);
  }
}

export const duplicateView = {
  render: async (root) => {
    root.innerHTML = `<section class="card"><div class="card-body">${loadingBlock('Preparing…')}</div></section>`;
    try {
      dup.folders = await api('/paths');
      if (!dup.roots.length && dup.folders.home) dup.roots = [`${dup.folders.home}\\Downloads`];
      paintDup(root);
      const input = root.querySelector('#dup-path');
      if (input) {
        input.dataset.downloads = `${dup.folders.home}\\Downloads`;
        input.dataset.pictures = `${dup.folders.home}\\Pictures`;
        input.dataset.documents = `${dup.folders.home}\\Documents`;
      }
    } catch (err) {
      root.innerHTML = `<section class="card"><div class="card-body"><div class="inline-note bad">${icon('alert')}<div>${esc(err.message)}</div></div></div></section>`;
    }
  },
};
