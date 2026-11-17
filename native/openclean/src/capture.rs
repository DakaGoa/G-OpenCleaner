//! Automated screen capture, used to review every screen without clicking.
//!
//! `OPENCLEAN_CAPTURE=<dir>` walks all 13 screens, gives each one time to load
//! its data, asks eframe for a screenshot of the next frame and writes a PNG.

use crate::app::{App, View};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

enum Phase {
    /// Letting background workers fill the screen before capturing.
    Settle,
    /// Screenshot requested, waiting for the image.
    Awaiting,
}

pub struct Run {
    views: Vec<View>,
    index: usize,
    phase: Phase,
    until: Instant,
    dir: PathBuf,
    saved: usize,
    prepared: bool,
    log: Vec<String>,
}

/// Opens one Custom Clean category for the tour shot, so the review shows the
/// read-only file list and not only collapsed rows. Capture-only code.
fn prepare_clean(app: &mut App) {
    use crate::app::PreviewState;

    let opts = app.clean_opts();
    let candidates: Vec<crate::core::model::Rule> = app
        .rules
        .iter()
        .filter(|r| r.applicable && !r.targets.is_empty())
        .take(4)
        .cloned()
        .collect();

    let mut fallback: Option<(String, crate::core::model::RulePreview)> = None;
    for rule in candidates {
        let preview =
            crate::core::scanner::preview_rule(&rule, &opts, crate::core::jobs::PREVIEW_ROWS);
        let filled = !preview.paths.is_empty();
        if fallback.is_none() {
            fallback = Some((rule.id.clone(), preview.clone()));
        }
        if filled {
            app.clean.expanded.insert(rule.id.clone());
            app.clean.previews.insert(rule.id, PreviewState::Ready(preview));
            return;
        }
    }
    if let Some((id, preview)) = fallback {
        app.clean.expanded.insert(id.clone());
        app.clean.previews.insert(id, PreviewState::Ready(preview));
    }
}

fn tour() -> Vec<View> {
    vec![
        View::Health,
        View::Clean,
        View::Browsers,
        View::Cloud,
        View::Disk,
        View::Duplicates,
        View::Quarantine,
        View::Elevated,
        View::Startup,
        View::Performance,
        View::Software,
        View::Drivers,
        View::Uninstaller,
        View::Stats,
        View::Settings,
    ]
}

fn settle_for(view: View) -> Duration {
    let secs = match view {
        View::Health => 22,
        View::Drivers => 20,
        View::Disk => 34,
        View::Software => 10,
        View::Duplicates => 5,
        View::Quarantine => 3,
        View::Elevated => 2,
        View::Cloud => 5,
        View::Performance => 4,
        View::Browsers => 4,
        View::Uninstaller => 3,
        View::Startup => 3,
        View::Clean => 3,
        View::Stats => 2,
        View::Settings => 2,
    };
    Duration::from_secs(secs)
}

fn slug(view: View) -> String {
    view.title()
        .to_lowercase()
        .replace(' ', "-")
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "")
}

pub fn start(dir: PathBuf) -> Run {
    let _ = std::fs::create_dir_all(&dir);
    let views = tour();
    let first = views[0];
    Run {
        views,
        index: 0,
        phase: Phase::Settle,
        until: Instant::now() + settle_for(first),
        dir,
        saved: 0,
        prepared: false,
        log: Vec::new(),
    }
}

fn write_png(path: &Path, image: &egui::ColorImage) -> Result<(), image::ImageError> {
    let [width, height] = image.size;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        bytes.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
    }
    image::save_buffer(
        path,
        &bytes,
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
    )
}

fn take_screenshot(ctx: &egui::Context) -> Option<std::sync::Arc<egui::ColorImage>> {
    ctx.input(|input| {
        input.raw.events.iter().find_map(|event| match event {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    })
}

fn request_screenshot(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
}

pub fn tick(app: &mut App, ctx: &egui::Context) {
    let Some(mut run) = app.capture.take() else {
        return;
    };

    let now = Instant::now();

    if !run.prepared {
        run.prepared = true;
        if run.views[run.index] == View::Clean {
            prepare_clean(app);
        }
        if run.views[run.index] == View::Drivers {
            // A real Windows Update report can be supplied for the screenshot;
            // the tour never presses Check on its own.
            if let Ok(path) = std::env::var("OPENCLEAN_CAPTURE_DRIVERS") {
                if let Ok(raw) = std::fs::read_to_string(Path::new(&path)) {
                    if let Ok(report) = serde_json::from_str::<crate::core::model::DriverUpdates>(&raw)
                    {
                        app.drivers.updates = Some(report);
                    }
                }
            }
        }
        if run.views[run.index] == View::Elevated {
            // The screenshot can be given a real report written by
            // `--elevated measure` instead of showing "Not measured" rows.
            if let Ok(path) = std::env::var("OPENCLEAN_CAPTURE_ELEVATED") {
                app.elevated.measured = crate::core::elevate::read_report(Path::new(&path));
            }
        }
    }

    // A screenshot from the previous request arrives as an input event.
    if matches!(run.phase, Phase::Awaiting) {
        if let Some(image) = take_screenshot(ctx) {
            let view = run.views[run.index];
            let file = run.dir.join(format!("{:02}-{}.png", run.index + 1, slug(view)));
            match write_png(&file, &image) {
                Ok(_) => {
                    run.saved += 1;
                    run.log
                        .push(format!("saved {} ({}x{})", file.display(), image.size[0], image.size[1]));
                }
                Err(err) => run.log.push(format!("failed {}: {}", file.display(), err)),
            }

            run.index += 1;
            if run.index >= run.views.len() {
                let _ = std::fs::write(run.dir.join("capture.log"), run.log.join("\n"));
                app.capture = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            app.set_view(run.views[run.index]);
            run.prepared = false;
            run.phase = Phase::Settle;
            run.until = now + settle_for(run.views[run.index]);
        } else if now >= run.until {
            // The backend never answered; record it and move on.
            run.log
                .push(format!("no image for {}", run.views[run.index].title()));
            run.index += 1;
            if run.index >= run.views.len() {
                let _ = std::fs::write(run.dir.join("capture.log"), run.log.join("\n"));
                app.capture = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            app.set_view(run.views[run.index]);
            run.prepared = false;
            run.phase = Phase::Settle;
            run.until = now + settle_for(run.views[run.index]);
        }
    } else if now >= run.until {
        request_screenshot(ctx);
        run.phase = Phase::Awaiting;
        run.until = now + Duration::from_secs(5);
    }

    app.capture = Some(run);
    ctx.request_repaint_after(Duration::from_millis(60));
}
