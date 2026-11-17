# G-OpenCleaner

A modern, open-source PC cleaner and maintenance suite for Windows — the feature set of a
commercial cleaner, rebuilt from scratch, released into the public domain, and with no
activation, upsells or telemetry.

## Native Windows app (Rust)

The product is a native Win32 window built with `eframe`/`egui` — no WebView2, no browser,
no Node at runtime.

```
build.cmd
```

produces:

| File | What it is |
| --- | --- |
| `dist\G-OpenCleanerSetup.exe` | Installer. Per-user, no administrator prompt. |
| `native\target\release\openclean.exe` | The application itself, runnable without installing. |

After installing, G-OpenCleaner is in the Start menu, on the Desktop, and in
Settings → Apps as **G-OpenCleaner 1.0.0** with a working uninstall command. It installs to
`%LOCALAPPDATA%\Programs\G-OpenCleaner`; the uninstaller is copied beside it as
`Uninstall.exe` and accepts `--uninstall --quiet`.

Build requirements: Rust 1.75+ with the MSVC toolchain.

Safety self-test, which writes `%LOCALAPPDATA%\OpenClean\selftest.txt`:

```
openclean.exe --self-test
```

It asserts that the deletion guard rejects `%WINDIR%\System32`, the Windows folder itself,
drive roots, `Program Files`, Documents and G-OpenCleaner's own settings; that it accepts
`%WINDIR%\MEMORY.DMP` and files in `%TEMP%`; that exclusions are honoured; and that every
resolved target of all 69 catalogue rules sits inside the allowlist (2,309 targets, 0
outside).

It also exercises the quarantine: a file is moved in and its original path kept, restored with
its contents intact, purged once past its retention, and both a protected file and a crafted
manifest pointing into `System32` are refused.

Review-only screen capture, for checking every screen visually from a real build. It is a
cargo feature and is **not** part of the shipped binary:

```
cargo build --release -p openclean --features capture
OPENCLEAN_CAPTURE=<folder> openclean.exe      # one PNG per screen, then exits
```

## Web version (kept as the reference implementation)

The same feature set also exists as a zero-dependency Node app, which is what the native
build was ported from:

```
Double-click  start.cmd        (or:  npm start)
```

Then open the printed address, normally <http://127.0.0.1:7317>.

Requires Node.js 18+ and Windows 10/11. Zero npm dependencies.

---

## 1. How CCleaner 7 actually works (reverse-engineering notes)

The copy analysed lives in `C:\Program Files\Piriform\CCleaner 7`. It is not a classic MFC
application anymore — it is a **WebView2 shell around a rules engine**:

| Piece | What it does |
| --- | --- |
| `CCleaner.exe` (7.6 MB) | Thin WebView2 host: loads `gui/index.html`, brokers messages between the UI and the service. |
| `gui/` (Svelte bundle) | The entire interface. `index.html` preloads `chunks/*.js`; navigation labels live in `chunks/en.js`. |
| `CCleaner_service.exe` (32 MB) | Privileged worker that performs scans, deletions and registry edits. |
| `ccleaner.ini` (11,027 lines) | The cleaning rule catalogue — ~600 `[App]` sections in a small DSL. |
| `data/config.def` | Server-driven feature flags, marketing campaigns and notification limits. |
| `data/*.db` | SQLite state: `statistics.db`, `CCleanerPO.db`, backup storage. |
| `licensing.log`, `MySubscription.js` | Activation, entitlement and upsell layer. |

The rule DSL is the interesting part. Each section is data:

```ini
[Temporary Files]
Author=CCleaner
ID=windows.tempfiles
Default=True
FileKey1=%windir%|*.old;*.tmp
FileKey3=%TMP%|*|RECURSE
```

`FileKeyN = <path>|<pattern>|<flags>`, plus `RegKeyN` for registry values, `DetectOS` for
version gating, and `ActionN` for named operations (`DiskCleanup|Thumbnail Cache`,
`ShellAddToRecentDocs`). Rules are grouped, have a default on/off state, and every path is
built from `%VAR%` tokens expanded at runtime.

**Feature set** (from `gui/chunks/en.js` navigation keys, one JS bundle each):

Health Check · Custom Clean · Browser Cleaner · Cloud Drive Cleaner · Duplicate Finder ·
Software Updater · Driver Updater · Performance Optimizer · Uninstaller · Startup Manager ·
Statistics — plus Settings, and a disk/drive analyser used inside Health Check.

## 2. What G-OpenCleaner is

The same eleven modules, implemented for real:

| Module | Implementation |
| --- | --- |
| **Health Check** | Scans junk, privacy traces, startup count, disk headroom, memory pressure and driver age; produces a weighted 0–100 score and one "fix" action. The fix set contains only re-creatable data — see safety point 7. |
| **Custom Clean** | 50+ rules across Windows, applications and browsers. Per-category selection, search, analyse, then clean with a size breakdown. Any category expands to the largest files it would delete — read-only, capped at 40 paths plus the true totals, and scanned only when you open the row. |
| **Browser Cleaner** | Detects Chrome, Edge, Brave, Vivaldi, Opera, Opera GX and Firefox; separates cache, cookies, history, sessions, form data and passwords. |
| **Cloud Drive Cleaner** | Finds OneDrive, Dropbox, Google Drive, iCloud, pCloud and Nextcloud folders; reports files untouched for a year, files over 100 MB, and empty folders. |
| **Disk Analyzer** | Single-pass size bucketing at 1–3 levels with drill-down, plus the largest files on the volume. |
| **Duplicate Finder** | Three-stage matching: size → first 64 KiB hash → full MD5. Keeps the oldest copy by default. |
| **Startup Manager** | HKCU/HKLM Run keys and startup folders; disable moves the entry to a backup key so it is restorable, not destroyed. |
| **Performance Optimizer** | Live per-process CPU (two-sample delta) and working set, sortable, with process termination and third-party scheduled tasks. |
| **Software Updater** | Reads the Uninstall registry hives; checks and applies updates through `winget` when present. |
| **Driver Updater** | Enumerates signed drivers through WMI, flags anything older than three years, and creates a restore point on request. A read-only Windows Update Agent search — only when you press Check — lists the drivers Windows Update offers, matches them to installed devices by hardware ID and opens Windows Update to install them. Nothing is downloaded by the app. |
| **Uninstaller** | Full installed-program inventory (HKLM, WOW6432Node, HKCU) and launches each vendor's own uninstaller. |
| **Statistics** | Lifetime bytes, files, run count and a per-run history chart, stored locally. |
| **Administrator Tasks** | The two cleanups a standard user cannot run: removing `Windows.old` and cleaning the component store with DISM. Each is measured first so you confirm against a real size, runs in a second copy of the app behind the Windows UAC prompt, and offers a restore point first. |
| **Quarantine** | The undo journal: every removed file, its original path and size, with Restore and Purge. Files leave it after seven days — see safety point 9. |

### Deliberate differences from the original

- **No licence, no account, no upsell.** Every feature is unlocked. No activation service, no
  "premium" module, no offer banners, no telemetry.
- **No bundled third-party installers** (the original ships Avast/AVG campaign URLs in
  `config.def`; none of that exists here).
- **No remote driver database.** Driver Updater reports what Windows reports. Shipping a
  stale third-party driver catalogue would be less honest and more dangerous than pointing
  you at your hardware vendor.
- **Local web UI instead of WebView2.** The backend does the privileged work, the browser is
  only a view. This keeps the app auditable — `view-source` on a real cleaner.

## 3. Safety model

The parts that delete things are the parts worth reviewing:

1. **Path allowlist.** `server/scanner.js` (and `native/openclean/src/core/scanner.rs`)
   only permits deletion strictly *inside* disposable roots: `%TEMP%`, `%LOCALAPPDATA%`, `%APPDATA%`, `%ProgramData%`, `%WINDIR%\Temp`,
   `%WINDIR%\Logs`, `%WINDIR%\Prefetch`, `%WINDIR%\SoftwareDistribution\Download`, and so on.
   A rule cannot opt out of the check.
2. **Never the root itself.** `isInside()` returns false for an exact match, so a bug cannot
   turn "empty this folder" into "delete this folder".
3. **No reparse traversal.** Junctions and symlinks are skipped in both scanning and deletion,
   which prevents OneDrive/Dropbox recursion loops and escaping the allowlist.
4. **Explicit confirmation.** Destructive endpoints require `confirm: true` in the body and
   reject the request otherwise — the setting is enforced server-side, not in the UI.
5. **Session token.** The server binds `127.0.0.1` only and injects a random per-run token
   into the page; API calls without it get 403, so another local page cannot drive deletions.
6. **Skip, never force.** Locked or in-use files are counted as skipped. Nothing is queued for
   deletion at reboot behind your back.
7. **One-click fix never destroys user data.** Rules flagged `healthFix: false` are excluded
   from the Health Check scan and fix plan: the Recycle Bin (`permanent: true`) and the
   clipboard. Emptying the bin is irreversible, so it is only ever an explicit choice in
   Custom Clean, where the row is badged *permanent* and the confirmation says what is lost.
   The Health Check dialog lists every category and every system action it will run, so the
   copy and the work always match.

8. **System locations are untouchable.** A protection list is checked *before* the allowlist,
   for every deletion, whatever calls it: `%WINDIR%\System32`, `SysWOW64`, `WinSxS`, `Boot`,
   `System32\config|drivers|catroot|DriverStore|spool`, `%ProgramFiles%`, ProgramData's
   `Crypto`, `Protect`, `SystemData`, `Package Cache` and Defender folders, the Start menu
   folders, per-user `Crypto`/`Protect`/certificate stores, `%LOCALAPPDATA%\Programs`,
   G-OpenCleaner's own data, plus the names that must never vanish (`ntoskrnl`, `bootmgr`, `BCD`,
   `pagefile.sys`, `NTUSER.DAT`, the registry hives) and the `.sys`/`.efi`/`.drv` file types.
   A caller-supplied root — what the Disk Analyzer, Duplicate Finder and Cloud Drive Cleaner
   pass in — is only honoured for ordinary user data, never a system tree or a drive root.
   Core Windows processes (`csrss`, `winlogon`, `lsass`, `services`, `svchost`, …) cannot be
   ended from the Performance Optimizer either: the guard sits with the action, not just the
   button.

   Your own folders are protected the same way. `Documents`, `Desktop`, `Pictures`, `Videos`
   and `Music` are read from the Windows shell folders (so a OneDrive redirect is covered too)
   and can never be deleted from — not by a rule, and not by choosing their parent folder in
   the Disk Analyzer or Duplicate Finder. Those screens filter the selection, tell you how many
   items were skipped, and the deleter re-checks every path anyway. `Downloads` stays cleanable,
   since that is what a space tool is usually for.

9. **Deleting is reversible for a week.** Every rule that is not *permanent* moves files into
   `%LOCALAPPDATA%\OpenClean\Quarantine\<run>\files\…` instead of destroying them. The folder
   mirrors the original path, `manifest.json` records where each file came from and how big it
   was, and the **Quarantine** screen puts any file back at exactly the path it had, or purges
   it early. Runs older than seven days are purged when the app starts. Permanent rules
   (emptying the Recycle Bin), files you pick yourself in the Disk Analyzer, Duplicate Finder
   or Cloud Drive Cleaner, and secure delete are still destroyed outright. If a file cannot be
   stored it is left where it is rather than deleted, and a restore into a protected location —
   even from a hand-edited manifest pointing at `System32` — is refused by the same guard the
   cleaner uses.

10. **Administrator work is a separate process.** Nothing in this app elevates itself. The
    Administrator Tasks screen starts a second copy with the shell's `runas` verb — that is
    the UAC prompt — and that copy does the work and writes a JSON report the window reads.
    Dismissing the prompt changes nothing. Only two tasks exist: deleting the one folder
    `<systemdrive>\Windows.old` (the path guard accepts no parent, child or lookalike) and
    running DISM's `StartComponentCleanup /ResetBase`; both are measured first so the
    confirmation shows a real size, both warn about what they make impossible, and unless you
    untick it a restore point is created before anything is touched — if System Protection is
    off, the task stops there instead of going ahead.

## 4. Project layout

```
native/
  Cargo.toml        workspace (openclean + openclean-setup)
  openclean/        the app: core/ (rules, scanner, win, tools, jobs), views/, app.rs
  openclean-setup/  installer + uninstaller, embeds the app exe through build.rs
server/
  index.js      loopback HTTP server, token injection, static assets
  api.js        all endpoints + health scoring
  rules.js      cleaning rule catalogue (data only)
  scanner.js    %VAR% expansion, globbing, scan, clean, path safety
  win.js        PowerShell/WMI/registry bridge
  tools.js      disk analyzer, duplicate finder, cloud inspection
  store.js      settings + statistics persistence
public/
  index.html    shell
  css/app.css   design system (dark + light)
  js/core.js    fetch, formatting, toasts, dialogs, icons
  js/app.js     navigation + router
  js/views/     health, clean, storage, manage, updates, system
data/store.json  created on first write
```

## 5. Notes on what is measured vs. displayed

Everything shown is read live from this machine. There is no demo data, no mocked scan
results and no fake progress. If a value cannot be measured (no `winget`, no cloud client,
no fixed drive), the UI says so instead of inventing a number.

## Licence

Public domain — [Unlicense](LICENSE). No attribution required, no restrictions.
