//! Cleaning rule catalogue. Data only — the scanner decides what happens.

use super::model::{Action, Group, Mode, Rule, Target};

struct BrowserDef {
    id: &'static str,
    name: &'static str,
    base: &'static str,
}

const CHROMIUM: &[BrowserDef] = &[
    BrowserDef { id: "chrome", name: "Google Chrome", base: r"%LOCALAPPDATA%\Google\Chrome\User Data" },
    BrowserDef { id: "edge", name: "Microsoft Edge", base: r"%LOCALAPPDATA%\Microsoft\Edge\User Data" },
    BrowserDef { id: "brave", name: "Brave", base: r"%LOCALAPPDATA%\BraveSoftware\Brave-Browser\User Data" },
    BrowserDef { id: "vivaldi", name: "Vivaldi", base: r"%LOCALAPPDATA%\Vivaldi\User Data" },
    BrowserDef { id: "opera", name: "Opera", base: r"%APPDATA%\Opera Software\Opera Stable" },
    BrowserDef { id: "operagx", name: "Opera GX", base: r"%APPDATA%\Opera Software\Opera GX Stable" },
];

const CACHE_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    r"Service Worker\CacheStorage",
    "DawnCache",
    "GrShaderCache",
];

fn rule(
    id: impl Into<String>,
    group: Group,
    name: impl Into<String>,
    desc: impl Into<String>,
    default_on: bool,
    targets: Vec<Target>,
) -> Rule {
    Rule {
        id: id.into(),
        group,
        name: name.into(),
        desc: desc.into(),
        default_on,
        health_fix: true,
        permanent: false,
        warning: false,
        browser: None,
        browser_id: None,
        targets,
        applicable: false,
    }
}

fn browser_rule(
    browser: &BrowserDef,
    key: &str,
    name: &str,
    paths: Vec<String>,
    mode: Mode,
    default_on: bool,
    warning: bool,
) -> Rule {
    let mut built = rule(
        format!("browser.{}.{}", browser.id, key),
        Group::Browsers,
        name,
        format!("{} stored by {}.", name, browser.name),
        default_on,
        paths
            .into_iter()
            .map(|p| match mode {
                Mode::File => Target::file(p),
                _ => Target::dir(p),
            })
            .collect(),
    );
    built.browser = Some(browser.name);
    built.browser_id = Some(browser.id);
    built.warning = warning;
    built
}

fn chromium_rules() -> Vec<Rule> {
    let mut out = Vec::new();

    for browser in CHROMIUM {
        let profile = if browser.id.starts_with("opera") {
            browser.base.to_string()
        } else {
            format!(r"{}\*", browser.base)
        };
        let p = |suffix: &str| format!(r"{}\{}", profile, suffix);

        out.push(browser_rule(
            browser,
            "cache",
            "Internet cache",
            CACHE_DIRS.iter().map(|d| p(d)).collect(),
            Mode::Contents,
            true,
            false,
        ));

        out.push(browser_rule(browser, "cookies", "Cookies", vec![p(r"Network\Cookies")], Mode::File, true, false));
        out.push(browser_rule(
            browser,
            "history",
            "Browsing history",
            vec![p("History"), p("Top Sites"), p("Visited Links"), p(r"Network\Shortcuts")],
            Mode::File,
            true,
            false,
        ));
        out.push(browser_rule(
            browser,
            "session",
            "Session and open tabs",
            vec![p("Current Session"), p("Current Tabs"), p("Last Session"), p("Last Tabs")],
            Mode::File,
            true,
            false,
        ));
        out.push(browser_rule(browser, "forms", "Saved form information", vec![p("Web Data")], Mode::File, true, false));
        out.push(browser_rule(
            browser,
            "passwords",
            "Saved passwords",
            vec![p("Login Data")],
            Mode::File,
            false,
            true,
        ));
    }
    out
}

fn firefox_rules() -> Vec<Rule> {
    const PROFILE: &str = r"%APPDATA%\Mozilla\Firefox\Profiles\*";
    let p = |suffix: &str| format!(r"{}\{}", PROFILE, suffix);

    let defs: [(&str, &str, Vec<String>, Mode, bool, bool); 8] = [
        ("cache", "Internet cache", vec![p("cache2")], Mode::Contents, true, false),
        ("startupcache", "Startup cache", vec![p("startupCache")], Mode::Contents, true, false),
        ("cookies", "Cookies", vec![p("cookies.sqlite")], Mode::File, true, false),
        ("history", "Browsing history", vec![p("places.sqlite")], Mode::File, true, false),
        ("forms", "Saved form information", vec![p("formhistory.sqlite")], Mode::File, true, false),
        ("session", "Session and open tabs", vec![p("sessionstore-backups")], Mode::Contents, true, false),
        ("downloads", "Download history", vec![p("downloads.sqlite")], Mode::File, true, false),
        ("passwords", "Saved passwords", vec![p("logins.json"), p("key4.db")], Mode::File, false, true),
    ];

    defs.into_iter()
        .map(|(key, name, paths, mode, default_on, warning)| {
            let mut r = rule(
                format!("browser.firefox.{}", key),
                Group::Browsers,
                name,
                format!("{} stored by Mozilla Firefox.", name),
                default_on,
                paths
                    .into_iter()
                    .map(|p| if mode == Mode::File { Target::file(p) } else { Target::dir(p) })
                    .collect(),
            );
            r.browser = Some("Mozilla Firefox");
            r.browser_id = Some("firefox");
            r.warning = warning;
            r
        })
        .collect()
}

pub fn all() -> Vec<Rule> {
    let mut v = Vec::new();

    let mut push = |mut r: Rule| {
        r.applicable = false;
        v.push(r);
    };

    /* ------------------------------------------------------------ Windows */
    push(rule(
        "windows.temp",
        Group::Windows,
        "Temporary files",
        "Scratch files left behind by installers, updates and applications.",
        true,
        vec![Target::dir(r"%TEMP%"), Target::dir(r"%WINDIR%\Temp")],
    ));
    push(rule(
        "windows.temp24",
        Group::Windows,
        "Temporary files older than 24 hours",
        "Only touches temp files nobody has used for a day, so running apps keep theirs.",
        false,
        vec![Target::recurse_old(r"%TEMP%", 24), Target::recurse_old(r"%WINDIR%\Temp", 24)],
    ));
    push(rule(
        "windows.updatecache",
        Group::Windows,
        "Windows Update cache",
        "Downloaded update packages already installed by Windows.",
        true,
        vec![
            Target::dir(r"%WINDIR%\SoftwareDistribution\Download"),
            Target::dir(r"%WINDIR%\SoftwareDistribution\DeliveryOptimization"),
        ],
    ));
    push(rule(
        "windows.errorreports",
        Group::Windows,
        "Windows error reports",
        "Crash dumps and diagnostic bundles queued for Microsoft.",
        true,
        vec![
            Target::dir(r"%ProgramData%\Microsoft\Windows\WER\ReportQueue"),
            Target::dir(r"%ProgramData%\Microsoft\Windows\WER\ReportArchive"),
            Target::dir(r"%ProgramData%\Microsoft\Windows\WER\Temp"),
            Target::dir(r"%LOCALAPPDATA%\Microsoft\Windows\WER"),
        ],
    ));
    push(rule(
        "windows.logs",
        Group::Windows,
        "Old log files",
        "Setup, servicing and application logs that are no longer needed.",
        true,
        vec![
            Target::recurse(r"%WINDIR%\Logs", "*.log"),
            Target::recurse(r"%WINDIR%\Logs", "*.etl"),
            Target::recurse(r"%WINDIR%\Panther", "*.log"),
            Target::recurse(r"%ProgramData%\Microsoft\Windows\WER\ReportArchive", "*.log"),
        ],
    ));
    push(rule(
        "windows.dumps",
        Group::Windows,
        "Memory dumps",
        "Kernel and application crash dumps, often hundreds of megabytes each.",
        true,
        vec![
            Target::file(r"%WINDIR%\MEMORY.DMP"),
            Target::dir(r"%WINDIR%\Minidump"),
            Target::dir(r"%WINDIR%\LiveKernelReports"),
            Target::dir(r"%LOCALAPPDATA%\CrashDumps"),
        ],
    ));
    push(rule(
        "windows.thumbnails",
        Group::Windows,
        "Thumbnail cache",
        "Preview images Explorer regenerates automatically.",
        true,
        vec![
            Target::recurse(r"%LOCALAPPDATA%\Microsoft\Windows\Explorer", "thumbcache_*.db"),
            Target::recurse(r"%LOCALAPPDATA%\Microsoft\Windows\Explorer", "iconcache_*.db"),
        ],
    ));
    push(rule(
        "windows.recent",
        Group::Windows,
        "Recent documents",
        "Jump-list shortcuts to files you opened recently.",
        true,
        vec![Target::dir(r"%APPDATA%\Microsoft\Windows\Recent")],
    ));
    push(rule(
        "windows.inetcache",
        Group::Windows,
        "Windows internet cache",
        "Temporary internet files kept by the Windows shell and legacy apps.",
        true,
        vec![
            Target::dir(r"%LOCALAPPDATA%\Microsoft\Windows\INetCache"),
            Target::dir(r"%LOCALAPPDATA%\Microsoft\Windows\WebCache"),
        ],
    ));
    push(rule(
        "windows.prefetch",
        Group::Windows,
        "Prefetch data",
        "App launch traces used to speed up start-up. Windows rebuilds them.",
        false,
        vec![Target::recurse(r"%WINDIR%\Prefetch", "*.pf")],
    ));
    push(rule(
        "windows.clipboard",
        Group::Windows,
        "Clipboard",
        "Whatever is currently copied to the clipboard.",
        true,
        vec![Target::action(Action::Clipboard)],
    ));
    push(rule(
        "windows.dns",
        Group::Network,
        "DNS cache",
        "Resolver entries. Flushing fixes stale or hijacked lookups.",
        true,
        vec![Target::action(Action::Dns)],
    ));
    push(rule(
        "windows.recyclebin",
        Group::Windows,
        "Recycle Bin",
        "Files you deleted that are still occupying disk space.",
        true,
        vec![Target::action(Action::RecycleBin)],
    ));

    /* ------------------------------------------------------- Applications */
    let apps: [(&str, &str, &str, bool, Vec<Target>); 12] = [
        (
            "app.discord",
            "Discord cache",
            "Cached avatars, emoji and message media.",
            true,
            vec![
                Target::dir(r"%APPDATA%\discord\Cache"),
                Target::dir(r"%APPDATA%\discord\Code Cache"),
                Target::dir(r"%APPDATA%\discord\GPUCache"),
            ],
        ),
        (
            "app.slack",
            "Slack cache",
            "Workspace media and service-worker caches.",
            true,
            vec![
                Target::dir(r"%APPDATA%\Slack\Cache"),
                Target::dir(r"%APPDATA%\Slack\Code Cache"),
                Target::dir(r"%APPDATA%\Slack\Service Worker\CacheStorage"),
            ],
        ),
        (
            "app.teams",
            "Microsoft Teams cache",
            "Meeting and chat cache for the classic Teams client.",
            true,
            vec![
                Target::dir(r"%APPDATA%\Microsoft\Teams\Cache"),
                Target::dir(r"%APPDATA%\Microsoft\Teams\Code Cache"),
                Target::dir(r"%APPDATA%\Microsoft\Teams\GPUCache"),
            ],
        ),
        (
            "app.vscode",
            "Visual Studio Code cache",
            "Extension host and renderer caches.",
            true,
            vec![
                Target::dir(r"%APPDATA%\Code\Cache"),
                Target::dir(r"%APPDATA%\Code\CachedData"),
                Target::dir(r"%APPDATA%\Code\GPUCache"),
                Target::dir(r"%APPDATA%\Code\logs"),
            ],
        ),
        (
            "app.spotify",
            "Spotify cache",
            "Downloaded track chunks and artwork cache.",
            true,
            vec![
                Target::dir(r"%LOCALAPPDATA%\Spotify\Storage"),
                Target::dir(r"%LOCALAPPDATA%\Spotify\Data"),
            ],
        ),
        (
            "app.office",
            "Microsoft Office cache",
            "Office document cache and leftover upload sessions.",
            true,
            vec![
                Target::dir(r"%LOCALAPPDATA%\Microsoft\Office\16.0\OfficeFileCache"),
                Target::dir(r"%LOCALAPPDATA%\Microsoft\Office\15.0\OfficeFileCache"),
            ],
        ),
        (
            "app.adobe",
            "Adobe media cache",
            "Premiere and After Effects conformed audio/video cache.",
            true,
            vec![Target::dir(r"%APPDATA%\Adobe\Common\Media Cache Files")],
        ),
        (
            "app.nvidia",
            "NVIDIA shader cache",
            "Compiled shaders that the driver rebuilds on demand.",
            true,
            vec![
                Target::dir(r"%LOCALAPPDATA%\NVIDIA\DXCache"),
                Target::dir(r"%LOCALAPPDATA%\NVIDIA\GLCache"),
                Target::dir(r"%LOCALAPPDATA%\NVIDIA Corporation\NV_Cache"),
            ],
        ),
        (
            "app.amd",
            "AMD shader cache",
            "DirectX shader cache built by the AMD driver.",
            true,
            vec![
                Target::dir(r"%LOCALAPPDATA%\AMD\DxCache"),
                Target::dir(r"%LOCALAPPDATA%\AMD\DxcCache"),
            ],
        ),
        (
            "app.dev",
            "Developer package caches",
            "npm and pip download caches. Safe, but packages must be downloaded again.",
            false,
            vec![
                Target::dir(r"%LOCALAPPDATA%\npm-cache\_cacache"),
                Target::dir(r"%LOCALAPPDATA%\pip\cache"),
            ],
        ),
        (
            "app.unity",
            "Unity cache",
            "Asset import and shader caches for the Unity editor.",
            false,
            vec![Target::dir(r"%LOCALAPPDATA%\Unity\cache")],
        ),
        (
            "app.java",
            "Java web cache",
            "Java plug-in and Web Start cache.",
            true,
            vec![Target::dir(r"%LOCALAPPDATA%\Sun\Java\Deployment\cache")],
        ),
    ];

    for (id, name, desc, default_on, targets) in apps {
        push(rule(id, Group::Applications, name, desc, default_on, targets));
    }

    v.extend(chromium_rules());
    v.extend(firefox_rules());

    // Health Check must not destroy user data, and neither must the clipboard
    // (it discards whatever the user copied).
    for r in v.iter_mut() {
        if r.id == "windows.clipboard" {
            r.health_fix = false;
        }
        if r.id == "windows.recyclebin" {
            r.health_fix = false;
            r.permanent = true;
        }
    }

    v
}
