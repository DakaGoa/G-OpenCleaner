'use strict';

/**
 * Cleaning rule catalogue.
 *
 * Every rule is data only. Modes:
 *   contents  - delete every child entry of the matched folder(s)
 *   recurse   - delete every file under the matched folder(s) matching `pattern`
 *   file      - delete the matched file(s)
 *   action    - a named system action handled by the cleaner
 *
 * Paths use %VARS% and may contain `*` segments, expanded by the scanner.
 */

const CHROMIUM = [
  { id: 'chrome', name: 'Google Chrome', base: '%LOCALAPPDATA%\\Google\\Chrome\\User Data' },
  { id: 'edge', name: 'Microsoft Edge', base: '%LOCALAPPDATA%\\Microsoft\\Edge\\User Data' },
  { id: 'brave', name: 'Brave', base: '%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data' },
  { id: 'vivaldi', name: 'Vivaldi', base: '%LOCALAPPDATA%\\Vivaldi\\User Data' },
  { id: 'opera', name: 'Opera', base: '%APPDATA%\\Opera Software\\Opera Stable' },
  { id: 'operagx', name: 'Opera GX', base: '%APPDATA%\\Opera Software\\Opera GX Stable' },
];

// Chromium cache folders carry no user data and can always be cleared.
const CACHE_DIRS = ['Cache', 'Code Cache', 'GPUCache', 'Service Worker\\CacheStorage', 'DawnCache', 'GrShaderCache'];

// Files that hold browsing traces. Removing them is what every cleaner does;
// the browser simply rebuilds them on next start.
const TRACE_FILES = {
  cookies: ['Network\\Cookies'],
  history: ['History', 'Top Sites', 'Visited Links', 'Network\\Shortcuts'],
  session: ['Current Session', 'Current Tabs', 'Last Session', 'Last Tabs'],
  forms: ['Web Data'],
  passwords: ['Login Data'],
  downloads: ['Network\\Network Persistent State'],
};

const browserRules = [];
for (const browser of CHROMIUM) {
  const profile = browser.id === 'opera' || browser.id === 'operagx' ? browser.base : `${browser.base}\\*`;
  const add = (key, name, parts, mode, extra = {}) => {
    browserRules.push({
      id: `browser.${browser.id}.${key}`,
      group: 'Browser cleaner',
      browser: browser.name,
      browserId: browser.id,
      name,
      desc: `${name} stored by ${browser.name}.`,
      default: extra.default !== undefined ? extra.default : true,
      targets: parts.map((p) => ({ path: `${profile}\\${p}`, mode })),
      ...extra,
    });
  };
  add('cache', 'Internet cache', CACHE_DIRS, 'contents');
  add('cookies', 'Cookies', TRACE_FILES.cookies, 'file');
  add('history', 'Browsing history', TRACE_FILES.history, 'file');
  add('session', 'Session and open tabs', TRACE_FILES.session, 'file');
  add('forms', 'Saved form information', TRACE_FILES.forms, 'file');
  add('passwords', 'Saved passwords', TRACE_FILES.passwords, 'file', { default: false, warning: true });
}

// Firefox keeps its reusable cache in a different place to its profile data.
const FIREFOX_PROFILE = '%APPDATA%\\Mozilla\\Firefox\\Profiles\\*';
const firefoxRules = [
  ['cache', 'Internet cache', [`${FIREFOX_PROFILE}\\cache2`], 'contents', true],
  ['startupcache', 'Startup cache', [`${FIREFOX_PROFILE}\\startupCache`], 'contents', true],
  ['cookies', 'Cookies', [`${FIREFOX_PROFILE}\\cookies.sqlite`, `%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\cache2`], 'file', true],
  ['history', 'Browsing history', [`${FIREFOX_PROFILE}\\places.sqlite`], 'file', true],
  ['forms', 'Saved form information', [`${FIREFOX_PROFILE}\\formhistory.sqlite`], 'file', true],
  ['session', 'Session and open tabs', [`${FIREFOX_PROFILE}\\sessionstore-backups`], 'contents', true],
  ['downloads', 'Download history', [`${FIREFOX_PROFILE}\\downloads.sqlite`], 'file', true],
  ['passwords', 'Saved passwords', [`${FIREFOX_PROFILE}\\logins.json`, `${FIREFOX_PROFILE}\\key4.db`], 'file', false],
].map(([key, name, paths, mode, def]) => ({
  id: `browser.firefox.${key}`,
  group: 'Browser cleaner',
  browser: 'Mozilla Firefox',
  browserId: 'firefox',
  name,
  desc: `${name} stored by Mozilla Firefox.`,
  default: def,
  warning: key === 'passwords',
  targets: paths.map((p) => ({ path: p, mode })),
}));

const rules = [
  /* ---------------------------------------------------------------- Windows */
  {
    id: 'windows.temp',
    group: 'Windows',
    name: 'Temporary files',
    desc: 'Scratch files left behind by installers, updates and applications.',
    default: true,
    targets: [
      { path: '%TEMP%', mode: 'contents' },
      { path: '%WINDIR%\\Temp', mode: 'contents' },
    ],
  },
  {
    id: 'windows.temp24',
    group: 'Windows',
    name: 'Temporary files older than 24 hours',
    desc: 'Only touches temp files nobody has used for a day, so running apps keep theirs.',
    default: false,
    targets: [
      { path: '%TEMP%', mode: 'recurse', pattern: '*', olderThanHours: 24 },
      { path: '%WINDIR%\\Temp', mode: 'recurse', pattern: '*', olderThanHours: 24 },
    ],
  },
  {
    id: 'windows.updatecache',
    group: 'Windows',
    name: 'Windows Update cache',
    desc: 'Downloaded update packages already installed by Windows.',
    default: true,
    targets: [
      { path: '%WINDIR%\\SoftwareDistribution\\Download', mode: 'contents' },
      { path: '%WINDIR%\\SoftwareDistribution\\DeliveryOptimization', mode: 'contents' },
    ],
  },
  {
    id: 'windows.errorreports',
    group: 'Windows',
    name: 'Windows error reports',
    desc: 'Crash dumps and diagnostic bundles queued for Microsoft.',
    default: true,
    targets: [
      { path: '%ProgramData%\\Microsoft\\Windows\\WER\\ReportQueue', mode: 'contents' },
      { path: '%ProgramData%\\Microsoft\\Windows\\WER\\ReportArchive', mode: 'contents' },
      { path: '%ProgramData%\\Microsoft\\Windows\\WER\\Temp', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\Microsoft\\Windows\\WER', mode: 'contents' },
    ],
  },
  {
    id: 'windows.logs',
    group: 'Windows',
    name: 'Old log files',
    desc: 'Setup, servicing and application logs that are no longer needed.',
    default: true,
    targets: [
      { path: '%WINDIR%\\Logs', mode: 'recurse', pattern: '*.log' },
      { path: '%WINDIR%\\Logs', mode: 'recurse', pattern: '*.etl' },
      { path: '%WINDIR%\\Panther', mode: 'recurse', pattern: '*.log' },
      { path: '%ProgramData%\\Microsoft\\Windows\\WER\\ReportArchive', mode: 'recurse', pattern: '*.log' },
    ],
  },
  {
    id: 'windows.dumps',
    group: 'Windows',
    name: 'Memory dumps',
    desc: 'Kernel and application crash dumps, often hundreds of megabytes each.',
    default: true,
    targets: [
      { path: '%WINDIR%\\MEMORY.DMP', mode: 'file' },
      { path: '%WINDIR%\\Minidump', mode: 'contents' },
      { path: '%WINDIR%\\LiveKernelReports', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\CrashDumps', mode: 'contents' },
    ],
  },
  {
    id: 'windows.thumbnails',
    group: 'Windows',
    name: 'Thumbnail cache',
    desc: 'Preview images Explorer regenerates automatically.',
    default: true,
    targets: [
      { path: '%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer', mode: 'recurse', pattern: 'thumbcache_*.db' },
      { path: '%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer', mode: 'recurse', pattern: 'iconcache_*.db' },
    ],
  },
  {
    id: 'windows.recent',
    group: 'Windows',
    name: 'Recent documents',
    desc: 'Jump-list shortcuts to files you opened recently.',
    default: true,
    targets: [{ path: '%APPDATA%\\Microsoft\\Windows\\Recent', mode: 'contents' }],
  },
  {
    id: 'windows.inetcache',
    group: 'Windows',
    name: 'Windows internet cache',
    desc: 'Temporary internet files kept by the Windows shell and legacy apps.',
    default: true,
    targets: [
      { path: '%LOCALAPPDATA%\\Microsoft\\Windows\\INetCache', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\Microsoft\\Windows\\WebCache', mode: 'contents' },
    ],
  },
  {
    id: 'windows.prefetch',
    group: 'Windows',
    name: 'Prefetch data',
    desc: 'App launch traces used to speed up start-up. Windows rebuilds them.',
    default: false,
    targets: [{ path: '%WINDIR%\\Prefetch', mode: 'recurse', pattern: '*.pf' }],
  },
  {
    id: 'windows.clipboard',
    group: 'Windows',
    name: 'Clipboard',
    desc: 'Whatever is currently copied to the clipboard.',
    default: true,
    // Health Check must not touch anything the user cannot get back, so this
    // stays a Custom Clean choice: clearing the clipboard discards its contents.
    healthFix: false,
    targets: [{ path: '', mode: 'action', action: 'clipboard' }],
  },
  {
    id: 'windows.dns',
    group: 'Network',
    name: 'DNS cache',
    desc: 'Resolver entries. Flushing fixes stale or hijacked lookups.',
    default: true,
    targets: [{ path: '', mode: 'action', action: 'dns' }],
  },
  {
    id: 'windows.recyclebin',
    group: 'Windows',
    name: 'Recycle Bin',
    desc: 'Files you deleted that are still occupying disk space.',
    default: true,
    // Permanent: this destroys files the user could otherwise restore. It is
    // never part of Health Check's one-click fix, only an explicit choice.
    healthFix: false,
    permanent: true,
    targets: [{ path: '', mode: 'action', action: 'recyclebin' }],
  },

  /* --------------------------------------------------------- Applications */
  {
    id: 'app.discord',
    group: 'Applications',
    name: 'Discord cache',
    desc: 'Cached avatars, emoji and message media.',
    default: true,
    targets: [
      { path: '%APPDATA%\\discord\\Cache', mode: 'contents' },
      { path: '%APPDATA%\\discord\\Code Cache', mode: 'contents' },
      { path: '%APPDATA%\\discord\\GPUCache', mode: 'contents' },
    ],
  },
  {
    id: 'app.slack',
    group: 'Applications',
    name: 'Slack cache',
    desc: 'Workspace media and service-worker caches.',
    default: true,
    targets: [
      { path: '%APPDATA%\\Slack\\Cache', mode: 'contents' },
      { path: '%APPDATA%\\Slack\\Code Cache', mode: 'contents' },
      { path: '%APPDATA%\\Slack\\Service Worker\\CacheStorage', mode: 'contents' },
    ],
  },
  {
    id: 'app.teams',
    group: 'Applications',
    name: 'Microsoft Teams cache',
    desc: 'Meeting and chat cache for the classic Teams client.',
    default: true,
    targets: [
      { path: '%APPDATA%\\Microsoft\\Teams\\Cache', mode: 'contents' },
      { path: '%APPDATA%\\Microsoft\\Teams\\Code Cache', mode: 'contents' },
      { path: '%APPDATA%\\Microsoft\\Teams\\GPUCache', mode: 'contents' },
    ],
  },
  {
    id: 'app.vscode',
    group: 'Applications',
    name: 'Visual Studio Code cache',
    desc: 'Extension host and renderer caches.',
    default: true,
    targets: [
      { path: '%APPDATA%\\Code\\Cache', mode: 'contents' },
      { path: '%APPDATA%\\Code\\CachedData', mode: 'contents' },
      { path: '%APPDATA%\\Code\\GPUCache', mode: 'contents' },
      { path: '%APPDATA%\\Code\\logs', mode: 'contents' },
    ],
  },
  {
    id: 'app.spotify',
    group: 'Applications',
    name: 'Spotify cache',
    desc: 'Downloaded track chunks and artwork cache.',
    default: true,
    targets: [
      { path: '%LOCALAPPDATA%\\Spotify\\Storage', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\Spotify\\Data', mode: 'contents' },
    ],
  },
  {
    id: 'app.office',
    group: 'Applications',
    name: 'Microsoft Office cache',
    desc: 'Office document cache and leftover upload sessions.',
    default: true,
    targets: [
      { path: '%LOCALAPPDATA%\\Microsoft\\Office\\16.0\\OfficeFileCache', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\Microsoft\\Office\\15.0\\OfficeFileCache', mode: 'contents' },
    ],
  },
  {
    id: 'app.adobe',
    group: 'Applications',
    name: 'Adobe media cache',
    desc: 'Premiere and After Effects conformed audio/video cache.',
    default: true,
    targets: [{ path: '%APPDATA%\\Adobe\\Common\\Media Cache Files', mode: 'contents' }],
  },
  {
    id: 'app.nvidia',
    group: 'Applications',
    name: 'NVIDIA shader cache',
    desc: 'Compiled shaders that the driver rebuilds on demand.',
    default: true,
    targets: [
      { path: '%LOCALAPPDATA%\\NVIDIA\\DXCache', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\NVIDIA\\GLCache', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\NVIDIA Corporation\\NV_Cache', mode: 'contents' },
    ],
  },
  {
    id: 'app.amd',
    group: 'Applications',
    name: 'AMD shader cache',
    desc: 'DirectX shader cache built by the AMD driver.',
    default: true,
    targets: [
      { path: '%LOCALAPPDATA%\\AMD\\DxCache', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\AMD\\DxcCache', mode: 'contents' },
    ],
  },
  {
    id: 'app.dev',
    group: 'Applications',
    name: 'Developer package caches',
    desc: 'npm and pip download caches. Safe, but packages must be downloaded again.',
    default: false,
    targets: [
      { path: '%LOCALAPPDATA%\\npm-cache\\_cacache', mode: 'contents' },
      { path: '%LOCALAPPDATA%\\pip\\cache', mode: 'contents' },
    ],
  },
  {
    id: 'app.unity',
    group: 'Applications',
    name: 'Unity cache',
    desc: 'Asset import and shader caches for the Unity editor.',
    default: false,
    targets: [{ path: '%LOCALAPPDATA%\\Unity\\cache', mode: 'contents' }],
  },
  {
    id: 'app.java',
    group: 'Applications',
    name: 'Java web cache',
    desc: 'Java plug-in and Web Start cache.',
    default: true,
    targets: [{ path: '%LOCALAPPDATA%\\Sun\\Java\\Deployment\\cache', mode: 'contents' }],
  },
].concat(browserRules, firefoxRules);

const groups = ['Windows', 'Applications', 'Browser cleaner', 'Network'];

module.exports = { rules, groups, CHROMIUM };
