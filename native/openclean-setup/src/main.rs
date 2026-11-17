//! G-OpenCleaner installer and uninstaller.
//!
//! Per-user install (no administrator prompt): the application binary is embedded
//! in this executable at build time, unpacked into %LOCALAPPDATA%\Programs\G-OpenCleaner,
//! given Start menu and desktop shortcuts, and registered under HKCU so Windows
//! Settings shows it in "Apps & features" with a working uninstall command.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{
    self, Align, Color32, FontId, Frame, Margin, RichText, Rounding, Stroke, Vec2,
};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const APP_NAME: &str = "G-OpenCleaner";
/// The application's earlier name, retired on upgrade so only one copy remains.
const LEGACY_APP_NAME: &str = "OpenClean";
const VERSION: &str = "1.0.0";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const BG: Color32 = Color32::from_rgb(0x0a, 0x0d, 0x14);
const PANEL: Color32 = Color32::from_rgb(0x12, 0x17, 0x22);
const PANEL_2: Color32 = Color32::from_rgb(0x16, 0x1c, 0x28);
const LINE: Color32 = Color32::from_rgb(0x25, 0x2e, 0x40);
const TEXT: Color32 = Color32::from_rgb(0xe9, 0xee, 0xf8);
const DIM: Color32 = Color32::from_rgb(0xa3, 0xb0, 0xc6);
const MUTE: Color32 = Color32::from_rgb(0x78, 0x85, 0x9c);
const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x8c, 0xff);
const GOOD: Color32 = Color32::from_rgb(0x2d, 0xd4, 0xa7);
const BAD: Color32 = Color32::from_rgb(0xff, 0x5f, 0x6d);

mod payload {
    /// The compiled application, copied to assets/ by build.cmd before this runs.
    pub const BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));
}

/* ------------------------------------------------------------------- paths */

fn local_app_data() -> PathBuf {
    PathBuf::from(
        std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into()),
    )
}

fn install_dir() -> PathBuf {
    local_app_data().join("Programs").join(APP_NAME)
}

fn app_exe() -> PathBuf {
    install_dir().join("G-OpenCleaner.exe")
}

fn uninstall_exe() -> PathBuf {
    install_dir().join("Uninstall.exe")
}

fn user_data_dir() -> PathBuf {
    local_app_data().join(APP_NAME)
}

fn start_menu_link() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{}.lnk", APP_NAME))
}

fn desktop_link() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join("Desktop")
        .join(format!("{}.lnk", APP_NAME))
}

fn is_installed() -> bool {
    app_exe().is_file()
}

/* ------------------------------------------------------------------ helpers */

fn ps_quote(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

fn run_hidden(program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

fn create_shortcut(link: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = link.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');\
         $s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';\
         $s.Description='{} PC cleaner';$s.Save()",
        ps_quote(link),
        ps_quote(target),
        ps_quote(target.parent().unwrap_or(Path::new("."))),
        ps_quote(target),
        APP_NAME
    );
    let output = run_hidden(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ],
    )
    .map_err(|e| format!("could not start PowerShell: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn write_uninstall_entry() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
            APP_NAME
        ))
        .map_err(|e| format!("registry: {}", e))?;

    let exe = app_exe().to_string_lossy().to_string();
    let dir = install_dir().to_string_lossy().to_string();
    let uninstall = uninstall_exe().to_string_lossy().to_string();
    let size = (payload::BYTES.len() / 1024).max(1) as u32;

    key.set_value("DisplayName", &format!("{} {}", APP_NAME, VERSION))
        .map_err(|e| e.to_string())?;
    key.set_value("DisplayVersion", &VERSION).map_err(|e| e.to_string())?;
    key.set_value("Publisher", &APP_NAME).map_err(|e| e.to_string())?;
    key.set_value("DisplayIcon", &exe).map_err(|e| e.to_string())?;
    key.set_value("InstallLocation", &dir).map_err(|e| e.to_string())?;
    key.set_value("UninstallString", &format!("\"{}\" --uninstall", uninstall))
        .map_err(|e| e.to_string())?;
    key.set_value(
        "QuietUninstallString",
        &format!("\"{}\" --uninstall --quiet", uninstall),
    )
    .map_err(|e| e.to_string())?;
    key.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
    key.set_value("EstimatedSize", &size).map_err(|e| e.to_string())?;
    Ok(())
}

fn remove_uninstall_entry() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        APP_NAME
    ));
}

/// Removes the install folder after this process exits, since a running exe
/// cannot delete itself. The work is handed to a throw-away batch file so the
/// paths never have to survive `cmd /C` quoting.
fn schedule_folder_removal(dir: &Path) {
    let bat = std::env::temp_dir().join(format!("openclean-remove-{}.bat", std::process::id()));
    let script = format!(
        "@echo off\r\nping -n 3 127.0.0.1 >nul\r\nrmdir /s /q \"{}\"\r\ndel /f /q \"%~f0\"\r\n",
        dir.display()
    );
    if fs::write(&bat, script).is_ok() {
        let _ = Command::new("cmd")
            .arg("/C")
            .arg(&bat)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

/* ------------------------------------------------------------------- action */

fn install(launch_after: bool) -> Result<(), String> {
    if payload::BYTES.is_empty() {
        return Err(
            "This installer was built without the application payload. Run build.cmd to produce a complete installer."
                .into(),
        );
    }

    let dir = install_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {}", dir.display(), e))?;

    let exe = app_exe();
    fs::write(&exe, payload::BYTES)
        .map_err(|e| format!("could not write {}: {}", app_exe().display(), e))?;

    // The uninstaller is this same executable, kept beside the app.
    if let Ok(me) = std::env::current_exe() {
        if me != uninstall_exe() {
            let _ = fs::copy(&me, uninstall_exe());
        }
    }

    create_shortcut(&start_menu_link(), &exe)?;
    create_shortcut(&desktop_link(), &exe)?;
    write_uninstall_entry()?;
    retire_legacy_install();

    if launch_after {
        let _ = Command::new(&exe).spawn();
    }
    Ok(())
}

/// The app was called OpenClean until this version. Installing under the new
/// name removes the old copy, its shortcuts and its Apps & features entry, so
/// no stale duplicate is left behind.
fn retire_legacy_install() {
    let old_dir = local_app_data().join("Programs").join(LEGACY_APP_NAME);
    if !old_dir.is_dir() {
        return;
    }
    let start_menu = PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{}.lnk", LEGACY_APP_NAME));
    let desktop = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join("Desktop")
        .join(format!("{}.lnk", LEGACY_APP_NAME));
    let _ = fs::remove_file(start_menu);
    let _ = fs::remove_file(desktop);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        LEGACY_APP_NAME
    ));
    schedule_folder_removal(&old_dir);
}

fn uninstall(remove_user_data: bool) -> Result<(), String> {
    let _ = fs::remove_file(start_menu_link());
    let _ = fs::remove_file(desktop_link());
    remove_uninstall_entry();

    if remove_user_data {
        let _ = fs::remove_dir_all(user_data_dir());
    }

    let dir = install_dir();
    if dir.is_dir() {
        schedule_folder_removal(&dir);
    }
    Ok(())
}

/* ----------------------------------------------------------------------- ui */

enum Stage {
    Ready,
    Uninstalling,
    Finished(String),
    Failed(String),
}

struct Setup {
    uninstall_mode: bool,
    launch_after: bool,
    remove_data: bool,
    installed: bool,
    stage: Stage,
}

impl Setup {
    fn new(uninstall_mode: bool) -> Self {
        Setup {
            uninstall_mode,
            launch_after: true,
            remove_data: false,
            installed: is_installed(),
            stage: Stage::Ready,
        }
    }
}

fn button(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(13.5).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, LINE))
            .rounding(Rounding::same(8.0))
            .min_size(Vec2::new(0.0, 34.0)),
    )
}

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(MUTE));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.0).color(DIM));
        });
    });
}

impl eframe::App for Setup {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(26.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(34.0, 34.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, Rounding::same(10.0), ACCENT);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "OC",
                        FontId::proportional(13.0),
                        Color32::WHITE,
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new(APP_NAME).size(17.0).strong().color(TEXT));
                        ui.label(
                            RichText::new(format!("Setup {}", VERSION))
                                .size(11.0)
                                .color(MUTE),
                        );
                    });
                });

                ui.add_space(16.0);

                Frame::none()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, LINE))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(Margin::same(18.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        if self.uninstall_mode {
                            ui.label(
                                RichText::new(format!("Remove {} from this PC", APP_NAME))
                                    .size(15.0)
                                    .strong()
                                    .color(TEXT),
                            );
                            ui.label(
                                RichText::new(
                                    "The application, its shortcuts and its Windows entry are removed. Your personal files are never touched.",
                                )
                                .size(12.0)
                                .color(DIM),
                            );
                            ui.add_space(10.0);
                            let mut remove_data = self.remove_data;
                            if ui
                                .checkbox(&mut remove_data, "Also delete saved settings and statistics")
                                .changed()
                            {
                                self.remove_data = remove_data;
                            }
                        } else {
                            ui.label(
                                RichText::new(format!("Install {} {}", APP_NAME, VERSION))
                                    .size(15.0)
                                    .strong()
                                    .color(TEXT),
                            );
                            ui.label(
                                RichText::new(
                                    "A native Windows cleaner. Public domain, no account, no licence key, no telemetry and no upsells.",
                                )
                                .size(12.0)
                                .color(DIM),
                            );
                            ui.add_space(12.0);
                            row(ui, "Installs to", &install_dir().to_string_lossy());
                            row(ui, "Shortcuts", "Start menu and Desktop");
                            row(ui, "Administrator rights", "Not required (per-user install)");
                            if payload::BYTES.is_empty() {
                                row(ui, "Payload", "missing — installer is incomplete");
                            } else {
                                row(
                                    ui,
                                    "Download size",
                                    &format!("{:.1} MB", payload::BYTES.len() as f64 / 1_048_576.0),
                                );
                            }

                            if self.installed {
                                ui.add_space(10.0);
                                ui.label(
                                    RichText::new(
                                        format!(
                                            "{} is already installed. Installing again replaces the program files and keeps your settings.",
                                            APP_NAME
                                        ),
                                    )
                                    .size(11.5)
                                    .color(MUTE),
                                );
                            }

                            ui.add_space(10.0);
                            let mut launch = self.launch_after;
                            if ui
                                .checkbox(
                                    &mut launch,
                                    &format!("Launch {} when setup finishes", APP_NAME),
                                )
                                .changed()
                            {
                                self.launch_after = launch;
                            }
                        }

                        ui.add_space(14.0);

                        match &self.stage {
                            Stage::Ready => {
                                ui.horizontal(|ui| {
                                    if self.uninstall_mode {
                                        if button(ui, "Remove", BAD.gamma_multiply(0.85), Color32::WHITE)
                                            .clicked()
                                        {
                                            match uninstall(self.remove_data) {
                                                Ok(_) => {
                                                    self.stage = Stage::Finished(
                                                        format!(
                                                            "{} was removed. You can close this window.",
                                                            APP_NAME
                                                        ),
                                                    );
                                                }
                                                Err(e) => self.stage = Stage::Failed(e),
                                            }
                                        }
                                    } else if button(ui, "Install now", ACCENT, Color32::WHITE)
                                        .clicked()
                                    {
                                        match install(self.launch_after) {
                                            Ok(_) => {
                                                self.stage = Stage::Finished(format!(
                                                    "Installed to {}",
                                                    install_dir().display()
                                                ));
                                                self.installed = true;
                                            }
                                            Err(e) => self.stage = Stage::Failed(e),
                                        }
                                    }
                                    if button(ui, "Close", Color32::TRANSPARENT, DIM).clicked() {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                });
                            }
                            Stage::Uninstalling => {
                                ui.add(egui::Spinner::new());
                            }
                            Stage::Finished(text) => {
                                let text = text.clone();
                                ui.label(RichText::new(text).size(12.5).color(GOOD));
                                ui.add_space(10.0);
                                if button(ui, "Close", ACCENT, Color32::WHITE).clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            }
                            Stage::Failed(text) => {
                                let text = text.clone();
                                ui.label(RichText::new(text).size(12.0).color(BAD));
                                ui.add_space(10.0);
                                if button(ui, "Close", Color32::TRANSPARENT, DIM).clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            }
                        }
                    });

                ui.add_space(12.0);
                ui.label(
                    RichText::new(
                        format!(
                            "{} only deletes inside disposable cache locations, and every removal is confirmed first.",
                            APP_NAME
                        ),
                    )
                    .size(11.0)
                    .color(MUTE),
                );
            });
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let uninstall_mode = args.iter().any(|a| a == "--uninstall");
    let quiet = args.iter().any(|a| a == "--quiet");

    if quiet {
        let result = if uninstall_mode {
            uninstall(false)
        } else {
            install(false)
        };
        std::process::exit(match result {
            Ok(_) => 0,
            Err(_) => 1,
        });
    }

    let height = if uninstall_mode { 330.0 } else { 430.0 };
    let window_title = if uninstall_mode {
        format!("{} — Uninstall", APP_NAME)
    } else {
        format!("{} Setup", APP_NAME)
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, height])
            .with_min_inner_size([520.0, 300.0])
            .with_resizable(false)
            .with_title(window_title.clone()),
        ..Default::default()
    };

    eframe::run_native(
        &window_title,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Setup::new(uninstall_mode)))
        }),
    )
}
