//! G-OpenCleaner — native Windows PC cleaner.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(feature = "capture")]
mod capture;
mod core;
mod theme;
mod views;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-test") {
        std::process::exit(self_test());
    }
    // The elevated copy of this executable: no window, one task, one report.
    if args.iter().any(|a| a == "--elevated") {
        std::process::exit(core::elevate::run_cli(&args));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 800.0])
            .with_min_inner_size([980.0, 620.0])
            .with_title("G-OpenCleaner"),
        ..Default::default()
    };

    eframe::run_native(
        "G-OpenCleaner",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// Safety self-test: proves the deletion guard rejects system locations, that
/// every catalogue rule lands inside an allowed root, and writes a report.
fn self_test() -> i32 {
    use crate::core::{quarantine, rules, scanner};
    use std::path::{Path, PathBuf};

    let mut lines = Vec::new();
    let mut failures = 0u32;

    let mut check = |label: &str, passed: bool| {
        if !passed {
            failures += 1;
        }
        lines.push(format!("[{}] {}", if passed { "PASS" } else { "FAIL" }, label));
    };

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());

    check(
        "rejects C:\\Windows\\System32",
        !scanner::is_safe_to_delete(Path::new(r"C:\Windows\System32"), &[]),
    );
    check(
        "rejects a file inside System32",
        !scanner::is_safe_to_delete(Path::new(r"C:\Windows\System32\drivers\etc\hosts"), &[]),
    );
    check(
        "rejects the Windows folder itself",
        !scanner::is_safe_container(Path::new(&windir), &[]),
    );
    check(
        "rejects a drive root",
        !scanner::is_safe_to_delete(Path::new(r"C:\"), &[]),
    );
    check(
        "rejects its own settings file",
        !scanner::is_safe_to_delete(
            &crate::core::store::data_dir().join("store.json"),
            &[],
        ),
    );
    check(
        "rejects a document in Documents",
        !scanner::is_safe_to_delete(
            &std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
                .join("Documents")
                .join("letter.docx"),
            &[],
        ),
    );
    check(
        "rejects Program Files",
        !scanner::is_safe_to_delete(
            Path::new(r"C:\Program Files\SomeApp\app.exe"),
            &[],
        ),
    );
    check(
        "accepts the Windows memory dump",
        scanner::is_safe_to_delete(Path::new(&format!(r"{}\MEMORY.DMP", windir)), &[]),
    );

    let temp = std::env::temp_dir();

    check(
        "accepts a file in %TEMP%",
        scanner::is_safe_to_delete(&temp.join("openclean-selftest.tmp"), &[]),
    );
    check(
        "honours exclusions",
        scanner::is_excluded(
            &temp.join("openclean-selftest.tmp"),
            &[temp.to_string_lossy().to_string()],
        ),
    );

    // Everything below is system-critical: losing any of it breaks Windows.
    let program_data = std::env::var("ProgramData").unwrap_or_default();
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let profile = std::env::var("USERPROFILE").unwrap_or_default();
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());

    let forbidden: Vec<(String, String)> = vec![
        (
            "kernel32.dll in System32".into(),
            format!(r"{}\System32\kernel32.dll", windir),
        ),
        (
            "a WinSxS component".into(),
            format!(r"{}\WinSxS\amd64_windows-shell\shell32.dll", windir),
        ),
        (
            "a signed driver".into(),
            format!(r"{}\System32\drivers\disk.sys", windir),
        ),
        (
            "the registry hive".into(),
            format!(r"{}\System32\config\SYSTEM", windir),
        ),
        (
            "the boot database".into(),
            format!(r"{}\Boot\BCD", windir),
        ),
        (
            "a system service profile outside its cache".into(),
            format!(
                r"{}\ServiceProfiles\LocalService\AppData\Local\Microsoft\Windows\UsrClass.dat",
                windir
            ),
        ),
        (
            "machine crypto keys".into(),
            format!(r"{}\Microsoft\Crypto\RSA\MachineKeys\abc", program_data),
        ),
        (
            "BitLocker system data".into(),
            format!(r"{}\Microsoft\Windows\SystemData\S-1-5-18", program_data),
        ),
        (
            "the installer package cache".into(),
            format!(r"{}\Package Cache\vc_redist.x64.exe", program_data),
        ),
        (
            "user crypto keys".into(),
            format!(r"{}\Microsoft\Crypto\RSA\S-1-5-21\key", appdata),
        ),
        (
            "the user certificate store".into(),
            format!(r"{}\Microsoft\SystemCertificates\My\Certificates\abc", appdata),
        ),
        (
            "G-OpenCleaner's own program files".into(),
            format!(r"{}\Programs\G-OpenCleaner\G-OpenCleaner.exe", local),
        ),
        (
            "the page file".into(),
            format!(r"{}\pagefile.sys", system_drive),
        ),
        (
            "the user profile hive".into(),
            format!(r"{}\NTUSER.DAT", profile),
        ),
    ];

    for (label, path) in &forbidden {
        check(
            &format!("refuses {}", label),
            !scanner::is_safe_to_delete(Path::new(path), &[]),
        );
    }

    // End-to-end: the deleter itself must refuse before touching the disk.
    let probe = format!(r"{}\System32\openclean-probe.tmp", windir);
    let plain = scanner::CleanOpts::default();
    let stats = scanner::remove_path(Path::new(&probe), &plain, None);
    check(
        "refuses to delete inside System32 at runtime",
        stats.skipped >= 1 && stats.removed == 0,
    );

    let unlocked = scanner::CleanOpts {
        secure: false,
        exclusions: Vec::new(),
        extra_roots: vec![std::path::PathBuf::from(&windir)],
    };
    let stats = scanner::remove_path(Path::new(&probe), &unlocked, None);
    check(
        "a system folder cannot be used as a deletion root",
        stats.skipped >= 1 && stats.removed == 0,
    );

    check(
        "rejects the Windows folder as an extra root",
        !scanner::is_safe_extra_root(Path::new(&windir)),
    );
    check(
        "rejects a drive root as an extra root",
        !scanner::is_safe_extra_root(Path::new(r"C:\")),
    );
    check(
        "rejects ProgramData as an extra root",
        !scanner::is_safe_extra_root(Path::new(&program_data)),
    );
    check(
        "accepts %TEMP% as an extra root",
        scanner::is_safe_extra_root(&temp),
    );
    check(
        "still allows a browser cache file",
        scanner::is_safe_to_delete(
            &std::path::PathBuf::from(&local)
                .join(r"Google\Chrome\User Data\Default\Cache\data_0"),
            &[],
        ),
    );
    check(
        "still allows a Windows temp file",
        scanner::is_safe_to_delete(&temp.join("openclean-selftest-2.tmp"), &[]),
    );
    // Personal folders, including the case the user reported: a chosen root
    // that happens to be their profile, with files inside Documents.
    for (label, folder) in [
        ("Documents", "Documents"),
        ("Desktop", "Desktop"),
        ("Pictures", "Pictures"),
        ("Videos", "Videos"),
        ("Music", "Music"),
    ] {
        let path = std::path::PathBuf::from(&profile).join(folder).join("file.bin");
        check(
            &format!("refuses a file in {}", label),
            !scanner::is_safe_to_delete(&path, &[]),
        );
        check(
            &format!("refuses a file in {} chosen as a root", label),
            !scanner::is_safe_to_delete(
                &path,
                &[std::path::PathBuf::from(&profile)],
            ),
        );
        check(
            &format!("rejects {} as an extra root", label),
            !scanner::is_safe_extra_root(
                &std::path::PathBuf::from(&profile).join(folder)
            ),
        );
    }

    check(
        "personal folders all resolve to protected paths",
        !scanner::personal_roots().is_empty()
            && scanner::personal_roots().iter().all(|p| scanner::is_protected(p)),
    );
    check(
        "still allows Downloads, the analyzer's default root",
        scanner::is_safe_to_delete(
            &std::path::PathBuf::from(&profile).join("Downloads").join("setup.exe"),
            &[std::path::PathBuf::from(&profile).join("Downloads")],
        ),
    );

    check(
        "refuses to end csrss.exe",
        crate::core::win::is_critical_process("csrss.exe"),
    );
    check(
        "refuses to end lsass.exe",
        crate::core::win::is_critical_process("LSASS.EXE"),
    );
    check(
        "refuses to end svchost.exe",
        crate::core::win::is_critical_process("svchost.exe"),
    );
    check(
        "still allows ending a normal app",
        !crate::core::win::is_critical_process("notepad.exe"),
    );
    check(
        "still allows the font cache (rebuildable)",
        scanner::is_safe_to_delete(
            Path::new(&format!(
                r"{}\ServiceProfiles\LocalService\AppData\Local\FontCache\FontCache.dat",
                windir
            )),
            &[],
        ),
    );

    /* ------------------------------------------------------------ quarantine */

    let qroot = temp.join("openclean-selftest-quarantine");
    let _ = std::fs::remove_dir_all(&qroot);
    std::env::set_var("OPENCLEAN_QUARANTINE_DIR", &qroot);

    let opts_plain = scanner::CleanOpts::default();
    let opts_secure = scanner::CleanOpts {
        secure: true,
        ..scanner::CleanOpts::default()
    };

    check(
        "quarantines ordinary rules",
        scanner::quarantines_flag(false, &opts_plain),
    );
    check(
        "permanent rules delete outright",
        !scanner::quarantines_flag(true, &opts_plain),
    );
    check(
        "secure delete bypasses quarantine",
        !scanner::quarantines_flag(false, &opts_secure),
    );

    let sample = temp.join("openclean-quarantine-sample.txt");
    let _ = std::fs::write(&sample, b"quarantine roundtrip");
    let sample_size = std::fs::metadata(&sample).map(|m| m.len()).unwrap_or(0);

    let mut q = quarantine::Quarantiner::new("self-test").expect("quarantine folder");
    let stored = q.store(&sample);
    let run_id = q.id().to_string();
    let stored_rel = q
        .entries()
        .first()
        .map(|e| e.stored.clone())
        .unwrap_or_default();
    let held = q.finish();

    check(
        "moves a file into quarantine",
        stored == Some(sample_size) && held == sample_size,
    );
    check("the original file is gone", !sample.exists());
    check(
        "keeps the original path inside the run",
        qroot.join(&run_id).join(&stored_rel).is_file(),
    );
    check(
        "totals and listing see the run",
        quarantine::totals().1 == 1 && quarantine::list().len() == 1,
    );

    let restored = quarantine::restore(&run_id, &stored_rel);
    check("restores the file", restored.is_ok());
    check(
        "the restored file has its content",
        std::fs::read(&sample)
            .map(|b| b == b"quarantine roundtrip")
            .unwrap_or(false),
    );
    check(
        "an emptied run disappears",
        quarantine::list().is_empty() && quarantine::totals().1 == 0,
    );

    let _ = std::fs::write(&sample, b"purge me");
    let mut q = quarantine::Quarantiner::new("purge-test").expect("quarantine folder");
    let _ = q.store(&sample);
    let held = q.finish();
    let freed = quarantine::purge_expired(0);
    check(
        "purges runs past their retention",
        held > 0 && freed == held && quarantine::totals().1 == 0,
    );

    // Guards around the quarantine itself.
    let protected_file = format!(r"{}\System32\drivers\etc\hosts", windir);
    let mut q = quarantine::Quarantiner::new("guard").expect("quarantine folder");
    let refused = q.store(Path::new(&protected_file)).is_none();
    let _ = q.finish();
    check("quarantine refuses protected files", refused);

    let q_probe = format!(r"{}\System32\openclean-q.txt", windir);
    let mut q = quarantine::Quarantiner::new("e2e-guard").expect("quarantine folder");
    let stats = scanner::remove_path(Path::new(&q_probe), &opts_plain, Some(&mut q));
    let _ = q.finish();
    check(
        "quarantine mode still refuses System32",
        stats.skipped >= 1 && stats.quarantined == 0,
    );

    let evil_id = "run-crafted";
    let evil_dir = qroot.join(evil_id);
    let _ = std::fs::create_dir_all(evil_dir.join("files"));
    let _ = std::fs::write(evil_dir.join("files").join("evil.bin"), b"x");
    let crafted = quarantine::Run {
        id: evil_id.into(),
        at: crate::core::store::now_secs(),
        label: "crafted".into(),
        bytes: 1,
        entries: vec![quarantine::Entry {
            original: format!(r"{}\System32\evil.dll", windir),
            stored: r"files\evil.bin".into(),
            size: 1,
        }],
    };
    let _ = std::fs::write(
        evil_dir.join("manifest.json"),
        serde_json::to_string_pretty(&crafted).unwrap_or_default(),
    );
    let evil = quarantine::restore(evil_id, r"files\evil.bin");
    check("refuses to restore into System32", evil.is_err());
    check(
        "the crafted file stays in quarantine",
        evil_dir.join("files").join("evil.bin").is_file(),
    );
    let _ = std::fs::remove_dir_all(&evil_dir);

    let rm = temp.join("openclean-rm-quarantine.txt");
    let _ = std::fs::write(&rm, b"rm");
    let mut q = quarantine::Quarantiner::new("rm").expect("quarantine folder");
    let stats = scanner::remove_path(&rm, &opts_plain, Some(&mut q));
    let _ = q.finish();
    check(
        "remove_path quarantines when asked",
        stats.quarantined > 0 && !rm.exists(),
    );

    let rm2 = temp.join("openclean-rm-plain.txt");
    let _ = std::fs::write(&rm2, b"rm");
    let stats = scanner::remove_path(&rm2, &opts_plain, None);
    check(
        "remove_path deletes when no quarantine is given",
        stats.removed == 1 && stats.quarantined == 0 && !rm2.exists(),
    );

    let _ = quarantine::purge_all();
    let _ = std::fs::remove_file(&sample);
    let _ = std::fs::remove_dir_all(&qroot);

    check(
        "a measured store with nothing to reclaim is not 'no size'",
        crate::views::elevated::reclaim_label(true, Some(0)) == "Nothing to reclaim"
            && crate::views::elevated::reclaim_label(true, Some(1024)) == "Run DISM cleanup"
            && crate::views::elevated::reclaim_label(true, None) == "No size to act on"
            && crate::views::elevated::reclaim_label(false, None) == "Measure first",
    );

    check(
        "a stalled Windows Update search is killed and reported",
        crate::core::win::ps_json_timeout(
            "Start-Sleep -Seconds 30",
            std::time::Duration::from_millis(400),
        )
        .is_err(),
    );

    // Windows Update drivers are matched to installed devices by hardware ID.
    check(
        "matches an update by full hardware ID",
        crate::core::model::driver_matches(
            &[r"PCI\VEN_8086&DEV_A170&SUBSYS_1234&REV_31".to_string()],
            "pci\\ven_8086&dev_a170&subsys_1234&rev_31",
        ),
    );
    check(
        "matches an update by vendor and device ID",
        crate::core::model::driver_matches(
            &[r"PCI\VEN_8086&DEV_A170&SUBSYS_1234&REV_31".to_string()],
            r"PCI\VEN_8086&DEV_A170&SUBSYS_9999&REV_05",
        ),
    );
    check(
        "does not match a different device",
        !crate::core::model::driver_matches(
            &[r"PCI\VEN_8086&DEV_A170".to_string()],
            r"PCI\VEN_8086&DEV_A171",
        ),
    );

    // Administrator tasks: the path guard and the numbers DISM reports.
    let old = crate::core::elevate::windows_old_path();
    let drive = Path::new(&windir)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(r"C:\"));
    check(
        "Windows.old sits beside the Windows folder",
        old == drive.join("Windows.old"),
    );
    check(
        "accepts only that Windows.old path",
        crate::core::elevate::is_windows_old(&old),
    );
    check(
        "refuses the Windows folder as Windows.old",
        !crate::core::elevate::is_windows_old(Path::new(&windir)),
    );
    check(
        "refuses a lookalike next to Windows.old",
        !crate::core::elevate::is_windows_old(&old.with_file_name("Windows.old.bak")),
    );
    check(
        "refuses a folder inside Windows.old",
        !crate::core::elevate::is_windows_old(&old.join("Windows\\System32")),
    );
    check(
        "a measurement with no numbers is not a success",
        !crate::core::elevate::measured_anything(&[])
            && crate::core::elevate::measured_anything(&[("store_actual".to_string(), 1)]),
    );
    check(
        "parses DISM sizes",
        crate::core::elevate::parse_size("1.5 GB") == Some(1_610_612_736)
            && crate::core::elevate::parse_size("2 KB") == Some(2048)
            && crate::core::elevate::parse_size("nonsense").is_none(),
    );

    let catalogue = rules::all();
    let audit = scanner::audit_rules(&catalogue);
    let outside: Vec<&(String, String, bool)> = audit.iter().filter(|row| !row.2).collect();

    lines.push(String::new());
    lines.push(format!(
        "Catalogue: {} rules, {} resolved targets, {} outside the allowlist",
        catalogue.len(),
        audit.len(),
        outside.len()
    ));
    lines.push(format!(
        "Personal folders protected: {}",
        scanner::personal_roots()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    for (id, path, _) in outside.iter().take(25) {
        lines.push(format!("  outside allowlist: {} -> {}", id, path));
    }
    failures += outside.len() as u32;

    lines.push(String::new());
    lines.push(if failures == 0 {
        format!("RESULT: all checks passed")
    } else {
        format!("RESULT: {} problem(s)", failures)
    });

    let report = lines.join("\r\n");
    let dir = crate::core::store::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("selftest.txt"), &report);
    // A GUI-subsystem binary has no stdout, so the file above is the report.
    #[cfg(debug_assertions)]
    println!("{}", report);

    if failures == 0 {
        0
    } else {
        1
    }
}
