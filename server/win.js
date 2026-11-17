'use strict';

/**
 * Windows access layer.
 * Every native operation goes through one of three places:
 *   - Node's own fs/os (fast, no shell)
 *   - PowerShell (structured data, registry, WMI)
 *   - A small number of well-known EXEs (reg.exe, winget.exe)
 */

const os = require('os');
const path = require('path');
const { execFile } = require('child_process');

function run(cmd, args, opts = {}) {
  return new Promise((resolve) => {
    execFile(
      cmd,
      args,
      {
        windowsHide: true,
        maxBuffer: 64 * 1024 * 1024,
        timeout: opts.timeout || 120000,
        encoding: 'utf8',
      },
      (err, stdout, stderr) => {
        resolve({
          ok: !err,
          stdout: stdout || '',
          stderr: stderr || '',
          error: err || null,
        });
      }
    );
  });
}

/** PowerShell single-quoted literal. */
function lit(value) {
  return "'" + String(value == null ? '' : value).replace(/'/g, "''") + "'";
}

function tidyError(text) {
  return String(text || '')
    .replace(/^#< CLIXML[\s\S]*$/m, '')
    .replace(/\r/g, '')
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .slice(0, 4)
    .join(' ');
}

async function ps(script, timeout) {
  const prelude =
    "[Console]::OutputEncoding=[Text.Encoding]::UTF8; " +
    "$ProgressPreference='SilentlyContinue'; " +
    "$ErrorActionPreference='Continue'; ";
  return run(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', prelude + script],
    { timeout }
  );
}

async function psJson(script, timeout) {
  const res = await ps(script, timeout);
  const out = res.stdout.trim();
  if (!out) {
    if (!res.ok) throw new Error(tidyError(res.stderr) || 'PowerShell command failed');
    return null;
  }
  const start = out.search(/[[{]/);
  const body = start >= 0 ? out.slice(start) : out;
  try {
    const parsed = JSON.parse(body);
    return Array.isArray(parsed) ? parsed : [parsed];
  } catch {
    if (!res.ok) throw new Error(tidyError(res.stderr) || 'PowerShell command failed');
    return null;
  }
}

/* ------------------------------------------------------------------ *
 * System information
 * ------------------------------------------------------------------ */

async function systemInfo() {
  const cpus = os.cpus() || [];
  const base = {
    hostname: os.hostname(),
    user: os.userInfo().username,
    platform: `${os.platform()} ${os.release()}`,
    arch: process.arch === 'x64' ? '64-bit' : process.arch,
    cpuModel: cpus.length ? cpus[0].model.trim() : 'Unknown CPU',
    cpuCores: cpus.length,
    memTotal: os.totalmem(),
    memFree: os.freemem(),
    uptime: os.uptime(),
    node: process.version,
    admin: false,
    osName: 'Windows',
    osVersion: os.release(),
    installDate: null,
    bootTime: null,
    gpu: [],
    battery: null,
  };

  try {
    const rows = await psJson(
      `$o = Get-CimInstance Win32_OperatingSystem;
       $cs = Get-CimInstance Win32_ComputerSystem;
       $enc = Get-CimInstance Win32_EncryptableVolume -ErrorAction SilentlyContinue;
       [pscustomobject]@{
         Caption = $o.Caption;
         Version = $o.Version;
         Build = $o.BuildNumber;
         InstallDate = if ($o.InstallDate) { $o.InstallDate.ToString('yyyy-MM-dd') } else { '' };
         BootTime = if ($o.LastBootUpTime) { $o.LastBootUpTime.ToString('yyyy-MM-dd HH:mm') } else { '' };
         RamTotal = [double]$cs.TotalPhysicalMemory;
         Admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);
         Gpu = (Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | Where-Object { $_.Name } | Select-Object -ExpandProperty Name) -join '|';
       } | ConvertTo-Json -Compress`,
      30000
    );
    const info = rows && rows[0];
    if (info) {
      base.osName = (info.Caption || 'Windows').replace(/\s+$/, '');
      base.osVersion = [info.Version, info.Build].filter(Boolean).join(' / ');
      base.installDate = info.InstallDate || null;
      base.bootTime = info.BootTime || null;
      base.admin = !!info.Admin;
      if (info.Gpu) base.gpu = String(info.Gpu).split('|').filter(Boolean);
    }
  } catch {
    /* keep os.* fallbacks */
  }

  try {
    base.drives = await drives();
  } catch {
    base.drives = [];
  }

  return base;
}

async function drives() {
  const rows = await psJson(
    `Get-CimInstance Win32_LogicalDisk -Filter "DriveType=3" -ErrorAction SilentlyContinue |
       Sort-Object DeviceID |
       ForEach-Object { [pscustomobject]@{
         letter = $_.DeviceID;
         label = $_.VolumeName;
         total = [double]$_.Size;
         free = [double]$_.FreeSpace;
         fs = $_.FileSystem;
       } } | ConvertTo-Json -Depth 3 -Compress`,
    30000
  );
  return rows || [];
}

async function isAdmin() {
  const rows = await psJson(
    `([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) | ConvertTo-Json -Compress`,
    20000
  );
  return !!(rows && rows[0] === true);
}

async function recycleBin() {
  try {
    const rows = await psJson(
      `$sh = New-Object -ComObject Shell.Application;
       $rb = $sh.NameSpace(0xA);
       if ($null -eq $rb) { [pscustomobject]@{ count = 0; size = 0 } | ConvertTo-Json -Compress }
       else {
         $items = @($rb.Items());
         $size = 0;
         foreach ($i in $items) { $size += [double]$i.Size }
         [pscustomobject]@{ count = $items.Count; size = $size; display = $rb.GetDetailsOf($null, 3) } | ConvertTo-Json -Compress
       }`,
      30000
    );
    return (rows && rows[0]) || { count: 0, size: 0 };
  } catch {
    return { count: 0, size: 0 };
  }
}

async function emptyRecycleBin() {
  const res = await ps('Clear-RecycleBin -Force -ErrorAction SilentlyContinue; $true | ConvertTo-Json', 60000);
  return { ok: res.ok, error: res.ok ? null : tidyError(res.stderr) };
}

/* ------------------------------------------------------------------ *
 * Installed software
 * ------------------------------------------------------------------ */

const UNINSTALL_KEYS = [
  { hive: 'HKLM', key: 'SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall' },
  { hive: 'HKLM', key: 'SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall' },
  { hive: 'HKCU', key: 'SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall' },
];

async function installedSoftware() {
  const roots = UNINSTALL_KEYS.map(
    (k) => `${k.hive === 'HKLM' ? 'HKLM:' : 'HKCU:'}\\${k.key.replace(/\\/g, '\\')}`
  );
  const listExpr = roots.map((r) => lit(r)).join(',');
  const rows = await psJson(
    `$roots = @(${listExpr});
     $out = New-Object System.Collections.ArrayList;
     foreach ($root in $roots) {
       if (-not (Test-Path $root)) { continue }
       foreach ($child in Get-ChildItem $root -ErrorAction SilentlyContinue) {
         $p = Get-ItemProperty $child.PSPath -ErrorAction SilentlyContinue;
         if (-not $p -or -not $p.DisplayName) { continue }
         if ($p.SystemComponent -eq 1) { continue }
         if ($p.ParentKeyName -and $p.ParentKeyName -notlike '*Uninstall*') { continue }
         $null = $out.Add([pscustomobject]@{
           id = $child.PSChildName;
           hive = if ($root -like 'HKCU*') { 'HKCU' } else { 'HKLM' };
           name = $p.DisplayName;
           version = $p.DisplayVersion;
           publisher = $p.Publisher;
           installDate = $p.InstallDate;
           size = $p.EstimatedSize;
           installLocation = $p.InstallLocation;
           uninstall = $p.UninstallString;
           quietUninstall = $p.QuietUninstallString;
           icon = $p.DisplayIcon;
         })
       }
     }
     $out | ConvertTo-Json -Depth 3 -Compress`,
    90000
  );

  return (rows || [])
    .filter((r) => r && r.name)
    .map((r) => ({
      id: `${r.hive}\\${r.id}`,
      key: r.id,
      hive: r.hive,
      name: String(r.name).trim(),
      version: r.version || '',
      publisher: r.publisher || '',
      installDate: formatRegDate(r.installDate),
      size: r.size ? Number(r.size) * 1024 : 0,
      location: r.installLocation || '',
      uninstall: r.uninstall || '',
      quietUninstall: r.quietUninstall || '',
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

function formatRegDate(value) {
  if (!value) return '';
  const s = String(value).trim();
  if (/^\d{8}$/.test(s)) return `${s.slice(0, 4)}-${s.slice(4, 6)}-${s.slice(6, 8)}`;
  return s;
}

/** Launch the vendor's own uninstaller. We never delete program files ourselves. */
async function launchUninstaller(command) {
  if (!command) throw new Error('This entry has no uninstall command.');
  if (!/^(msiexec|\/|"|[A-Za-z]:\\)/i.test(command.trim())) {
    throw new Error('Unrecognised uninstall command.');
  }
  const res = await ps(`Start-Process -FilePath 'cmd.exe' -ArgumentList '/c ${command.replace(/'/g, "''")}' -WindowStyle Normal; $true`, 20000);
  return { ok: res.ok, error: res.ok ? null : tidyError(res.stderr) };
}

/* ------------------------------------------------------------------ *
 * Startup items
 * ------------------------------------------------------------------ */

const STARTUP_KEYS = [
  { hive: 'HKCU', key: 'Software\\Microsoft\\Windows\\CurrentVersion\\Run' },
  { hive: 'HKLM', key: 'Software\\Microsoft\\Windows\\CurrentVersion\\Run' },
  { hive: 'HKLM', key: 'Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run' },
  { hive: 'HKCU', key: 'Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce' },
];

const BACKUP_KEY = 'Software\\OpenClean\\DisabledStartup';

async function startupItems() {
  const keyDefs = STARTUP_KEYS.map((k) => `${k.hive}|${k.key}`).join(',');
  const rows = await psJson(
    `$defs = ${lit(keyDefs)}.Split(',');
     $out = New-Object System.Collections.ArrayList;
     foreach ($d in $defs) {
       $parts = $d.Split('|');
       $root = if ($parts[0] -eq 'HKCU') { 'HKCU:' } else { 'HKLM:' };
       $full = "$root\\$($parts[1])";
       if (-not (Test-Path $full)) { continue }
       $item = Get-Item $full -ErrorAction SilentlyContinue;
       if (-not $item) { continue }
       foreach ($name in $item.GetValueNames()) {
         $val = $item.GetValue($name);
         $null = $out.Add([pscustomobject]@{
           name = $name;
           command = [string]$val;
           hive = $parts[0];
           key = $parts[1];
           kind = 'registry';
         })
       }
     }
     $out | ConvertTo-Json -Depth 3 -Compress`,
    45000
  );

  const items = (rows || []).filter((r) => r && r.name).map((r) => ({
    id: Buffer.from(`${r.hive}|${r.key}|${r.name}`).toString('base64url'),
    name: r.name,
    command: r.command,
    hive: r.hive,
    key: r.key,
    kind: 'registry',
    enabled: true,
    source: `${r.hive} \\ ${r.key.split('\\').pop()}`,
  }));

  // Startup folders
  const folders = [
    path.join(process.env.APPDATA || '', 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup'),
    path.join(process.env.ProgramData || 'C:\\ProgramData', 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup'),
  ];
  for (const folder of folders) {
    try {
      const names = require('fs').readdirSync(folder, { withFileTypes: true });
      for (const entry of names) {
        if (!entry.isFile()) continue;
        if (entry.name.toLowerCase() === 'desktop.ini') continue;
        items.push({
          id: Buffer.from(`folder|${path.join(folder, entry.name)}`).toString('base64url'),
          name: entry.name.replace(/\.(lnk|url|exe|bat|cmd)$/i, ''),
          command: path.join(folder, entry.name),
          hive: folder.includes('ProgramData') ? 'ALL USERS' : 'CURRENT USER',
          key: 'Startup folder',
          kind: 'folder',
          enabled: true,
          source: 'Startup folder',
        });
      }
    } catch {
      /* folder may not exist */
    }
  }

  const disabled = await disabledStartup();
  for (const d of disabled) items.push(d);

  return items;
}

async function disabledStartup() {
  try {
    const rows = await psJson(
      `$root = 'HKCU:\\${BACKUP_KEY}';
       if (-not (Test-Path $root)) { '[]' ; exit }
       $out = New-Object System.Collections.ArrayList;
       foreach ($child in Get-ChildItem $root -ErrorAction SilentlyContinue) {
         $p = Get-ItemProperty $child.PSPath -ErrorAction SilentlyContinue;
         if (-not $p) { continue }
         $null = $out.Add([pscustomobject]@{
           name = $p.OriginalName; command = $p.Command; hive = $p.Hive; key = $p.SourceKey;
         })
       }
       $out | ConvertTo-Json -Depth 3 -Compress`,
      30000
    );
    return (rows || []).filter((r) => r && r.name).map((r) => ({
      id: Buffer.from(`disabled|${r.name}|${r.hive}|${r.key}`).toString('base64url'),
      name: r.name,
      command: r.command,
      hive: r.hive,
      key: r.key,
      kind: 'registry',
      enabled: false,
      source: `${r.hive} \\ ${String(r.key).split('\\').pop()} (disabled)`,
    }));
  } catch {
    return [];
  }
}

async function setStartupEnabled(item, enabled) {
  const root = item.hive === 'HKCU' ? 'HKCU:' : 'HKLM:';
  const fullKey = `${root}\\${item.key}`;
  const backupId = Buffer.from(`${item.hive}|${item.key}|${item.name}`).toString('hex').slice(0, 32);

  if (enabled) {
    const script = `
      $b = 'HKCU:\\${BACKUP_KEY}\\${backupId}';
      if (-not (Test-Path $b)) { throw 'This item is no longer in the disabled list.' }
      $p = Get-ItemProperty $b;
      if (-not (Test-Path ${lit(fullKey)})) { New-Item -Path ${lit(fullKey)} -Force | Out-Null }
      Set-ItemProperty -Path ${lit(fullKey)} -Name $p.OriginalName -Value $p.Command;
      Remove-Item $b -Recurse -Force;
      $true | ConvertTo-Json`;
    const res = await ps(script, 30000);
    if (!res.ok) throw new Error(tidyError(res.stderr) || 'Could not re-enable this item.');
    return { ok: true };
  }

  const script = `
    $src = ${lit(fullKey)};
    $name = ${lit(item.name)};
    $item = Get-Item $src -ErrorAction Stop;
    $cmd = [string]$item.GetValue($name);
    if (-not $cmd) { throw 'Startup entry not found.' }
    $b = 'HKCU:\\${BACKUP_KEY}\\${backupId}';
    New-Item -Path $b -Force | Out-Null;
    Set-ItemProperty -Path $b -Name 'OriginalName' -Value $name;
    Set-ItemProperty -Path $b -Name 'Command' -Value $cmd;
    Set-ItemProperty -Path $b -Name 'Hive' -Value ${lit(item.hive)};
    Set-ItemProperty -Path $b -Name 'SourceKey' -Value ${lit(item.key)};
    Remove-ItemProperty -Path $src -Name $name -ErrorAction Stop;
    $true | ConvertTo-Json`;
  const res = await ps(script, 30000);
  if (!res.ok) {
    const err = tidyError(res.stderr);
    throw new Error(
      /denied|privilege|Access/i.test(err)
        ? 'Changing an all-users startup entry needs administrator rights. Restart OpenClean as administrator.'
        : err || 'Could not disable this item.'
    );
  }
  return { ok: true };
}

async function deleteStartupItem(item) {
  if (item.kind === 'folder') {
    const trash = path.join(os.homedir(), '.openclean', 'startup-backup');
    require('fs').mkdirSync(trash, { recursive: true });
    require('fs').renameSync(item.command, path.join(trash, path.basename(item.command)));
    return { ok: true, movedTo: trash };
  }
  const root = item.hive === 'HKCU' ? 'HKCU:' : 'HKLM:';
  const res = await ps(
    `Remove-ItemProperty -Path ${lit(`${root}\\${item.key}`)} -Name ${lit(item.name)} -ErrorAction Stop; $true | ConvertTo-Json`,
    30000
  );
  if (!res.ok) {
    const err = tidyError(res.stderr);
    throw new Error(/denied|privilege|Access/i.test(err) ? 'Requires administrator rights.' : err || 'Could not remove this entry.');
  }
  return { ok: true };
}

/* ------------------------------------------------------------------ *
 * Running processes
 * ------------------------------------------------------------------ */

async function processes() {
  const rows = await psJson(
    `Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
       Where-Object { $_.Name } |
       ForEach-Object { [pscustomobject]@{
         pid = [int]$_.ProcessId;
         name = $_.Name;
         path = $_.ExecutablePath;
         memory = [double]$_.WorkingSetSize;
         cpuTime = ([double]$_.UserModeTime) + ([double]$_.KernelModeTime);
       } } | ConvertTo-Json -Depth 3 -Compress`,
    45000
  );

  const list = (rows || []).filter((r) => r && r.pid > 4).map((r) => ({
    pid: r.pid,
    name: r.name,
    path: r.path || '',
    memory: r.memory || 0,
    cpu: 0,
  }));

  // Two quick samples give a usable CPU percentage.
  const sample = new Map(list.map((p) => [p.pid, p]));
  await new Promise((r) => setTimeout(r, 900));
  const rows2 = await psJson(
    `Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
       ForEach-Object { [pscustomobject]@{ pid = [int]$_.ProcessId; cpu = ([double]$_.UserModeTime) + ([double]$_.KernelModeTime) } } |
       ConvertTo-Json -Depth 3 -Compress`,
    45000
  );
  const prev = new Map((rows || []).map((r) => [r.pid, r.cpuTime || 0]));
  const cores = os.cpus().length || 1;
  for (const row of rows2 || []) {
    const p = sample.get(row.pid);
    if (!p) continue;
    const delta = (row.cpu || 0) - (prev.get(row.pid) || 0);
    p.cpu = Math.max(0, Math.min(100, (delta / 1e5) / 0.9 / cores * 100));
  }

  return list.sort((a, b) => b.cpu - a.cpu || b.memory - a.memory);
}

async function killProcess(pid) {
  const n = Number(pid);
  if (!Number.isInteger(n) || n <= 4) throw new Error('Invalid process id.');
  const res = await ps(`Stop-Process -Id ${n} -Force -ErrorAction Stop; $true | ConvertTo-Json`, 20000);
  if (!res.ok) {
    const err = tidyError(res.stderr);
    throw new Error(/denied|Access|privilege/i.test(err) ? 'That process is protected. Run OpenClean as administrator to end it.' : err || 'Could not end the process.');
  }
  return { ok: true };
}

/* ------------------------------------------------------------------ *
 * Drivers
 * ------------------------------------------------------------------ */

async function drivers() {
  const rows = await psJson(
    `Get-CimInstance Win32_PnPSignedDriver -ErrorAction SilentlyContinue |
       Where-Object { $_.DeviceName -and $_.DriverVersion } |
       ForEach-Object { [pscustomobject]@{
         name = $_.DeviceName;
         version = $_.DriverVersion;
         date = if ($_.DriverDate) { $_.DriverDate.ToString('yyyy-MM-dd') } else { '' };
         manufacturer = $_.Manufacturer;
         deviceClass = $_.DeviceClass;
         deviceId = $_.DeviceID;
       } } | ConvertTo-Json -Depth 3 -Compress`,
    60000
  );
  const seen = new Set();
  return (rows || [])
    .filter((r) => r && r.name)
    .map((r) => ({
      name: String(r.name).trim(),
      version: r.version || '',
      date: r.date || '',
      manufacturer: r.manufacturer || '',
      deviceClass: r.deviceClass || '',
      deviceId: r.deviceId || '',
    }))
    .filter((r) => {
      const key = `${r.name}|${r.version}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .sort((a, b) => (a.deviceClass || '').localeCompare(b.deviceClass || '') || a.name.localeCompare(b.name));
}
/* ------------------------------------------------------------------ *
 * Windows Update leftovers, winget
 * ------------------------------------------------------------------ */

async function wingetUpgrades() {
  const res = await run('winget.exe', [
    'list',
    '--upgrade-available',
    '--include-unknown',
    '--accept-source-agreements',
    '--disable-interactivity',
  ], { timeout: 120000 });

  if (!res.ok && !res.stdout) return { supported: false, items: [] };
  const lines = res.stdout.split(/\r?\n/);
  const headerIdx = lines.findIndex((l) => /\bName\b/.test(l) && /\bId\b/.test(l) && /\bVersion\b/.test(l));
  if (headerIdx < 0) return { supported: true, items: [] };

  const header = lines[headerIdx];
  const idCol = header.indexOf('Id');
  const verCol = header.indexOf('Version');
  const availCol = header.indexOf('Available');

  const items = [];
  for (let i = headerIdx + 1; i < lines.length; i++) {
    const line = lines[i];
    if (!line || /^-+$/.test(line.trim())) continue;
    if (/^(\d+)\s+upgrades?/.test(line.trim())) continue;
    if (line.length <= idCol) continue;
    const name = line.slice(0, idCol).trim();
    const id = line.slice(idCol, verCol).trim();
    const version = line.slice(verCol, availCol).trim();
    const available = line.slice(availCol).trim().split(/\s{2,}/)[0];
    if (!name || !id) continue;
    items.push({ name, id, version, available });
  }
  return { supported: true, items };
}

async function restorePoint(label) {
  const res = await ps(
    `Checkpoint-Computer -Description ${lit(label || 'OpenClean')} -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop; $true | ConvertTo-Json`,
    180000
  );
  if (!res.ok) {
    const err = tidyError(res.stderr);
    throw new Error(
      /denied|privilege|Access/i.test(err)
        ? 'Creating a restore point needs administrator rights.'
        : /disabled|frequency/i.test(err)
        ? 'System Restore is disabled or throttled on this PC. Enable System Protection first.'
        : err || 'Could not create a restore point.'
    );
  }
  return { ok: true };
}

async function scheduledTasks() {
  const rows = await psJson(
    `Get-ScheduledTask -ErrorAction SilentlyContinue |
       Where-Object { $_.TaskPath -notlike '\\Microsoft\\*' } |
       ForEach-Object { [pscustomobject]@{
         name = $_.TaskName;
         path = $_.TaskPath;
         state = [string]$_.State;
         author = $_.Author;
       } } | ConvertTo-Json -Depth 3 -Compress`,
    45000
  );
  return (rows || []).filter((r) => r && r.name).map((r) => ({
    name: r.name,
    path: r.path,
    state: r.state,
    author: r.author,
    id: Buffer.from(`${r.path}${r.name}`).toString('base64url'),
  }));
}

module.exports = {
  run,
  lit,
  ps,
  psJson,
  systemInfo,
  drives,
  isAdmin,
  recycleBin,
  emptyRecycleBin,
  installedSoftware,
  launchUninstaller,
  startupItems,
  setStartupEnabled,
  deleteStartupItem,
  processes,
  killProcess,
  drivers,
  wingetUpgrades,
  restorePoint,
  scheduledTasks,
};
