//! `live-capture-youtube-music` — stdout-first YouTube Music capture producer.
//!
//! Polls for a YouTube Music window by title prefix, computes the crop
//! rectangle for the player bar using CSS-based DPI-independent geometry,
//! and spawns `live-capture --mode crop` with the calculated coordinates.
//!
//! This binary is a drop-in replacement for `live-capture` in the pipeline:
//!
//! ```text
//! live-capture-youtube-music -t "YouTube Music - Nekomaru LiveUI" \
//!   | live-ws --mode video --server ws://host:3000/internal/streams/youtube-music
//! ```
//!
//! When the capture child exits (e.g. window closed), the binary re-polls
//! for the window and restarts.  The parent's stdout pipe stays open across
//! restarts, so downstream `live-ws` sees a seamless stream.

mod crop;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clap::Parser;
use windows::Win32::Foundation::HWND;

/// Minimum child lifetime before we consider it a "real" run.
/// If `live-capture` exits faster than this (e.g. broken pipe from dead
/// live-ws), we sleep before retrying to avoid a tight spin loop.
const RAPID_EXIT_THRESHOLD: Duration = Duration::from_secs(1);

/// Extra sleep after a rapid exit to prevent spin-looping.
const RAPID_EXIT_COOLDOWN: Duration = Duration::from_secs(5);

const LOG_PREFIX: &str = "[@youtube-music]";

// ── CLI ─────────────────────────────────────────────────────────────────────

/// Stdout-first YouTube Music capture producer.
///
/// Finds the YTM window by title prefix, computes the player bar crop rect
/// from CSS layout constants and actual DPI, and spawns live-capture in
/// crop mode.  Restarts automatically when the window disappears.
#[derive(Parser)]
#[command(name = "live-capture-youtube-music")]
struct CliArgs {
    /// Window title prefix to match (e.g. "YouTube Music - Nekomaru LiveUI").
    #[arg(short = 't', long)]
    title: String,

    /// Stream ID tag passed to live-capture for log output.
    #[arg(long, default_value = "youtube-music")]
    stream_id: String,

    /// Encoder frame rate (1-60).
    #[arg(long, default_value_t = 15, value_parser = clap::value_parser!(u32).range(1..=60))]
    fps: u32,

    /// Window polling interval in seconds.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u64).range(1..))]
    poll_interval: u64,
}

// ── Window discovery ────────────────────────────────────────────────────────

/// Find the first visible window whose title starts with `prefix`.
///
/// Uses the `enumerate-windows` crate for robust window filtering (visibility,
/// cloaked, owned-window checks).  Warns to stderr if multiple matches exist.
fn find_ytm_window(prefix: &str) -> Option<enumerate_windows::WindowInfo> {
    let windows = enumerate_windows::enumerate_windows();
    let mut matches: Vec<_> = windows
        .into_iter()
        .filter(|w| w.title.starts_with(prefix))
        .collect();

    match matches.len() {
        0 => None,
        1 => Some(matches.remove(0)),
        n => {
            log::warn!("{LOG_PREFIX} {n} windows match \"{prefix}\", using the first");
            Some(matches.remove(0))
        }
    }
}

// ── Child process spawning ──────────────────────────────────────────────────

/// Resolve the path to a sibling executable (same directory as the running
/// binary).  All workspace binaries are co-located in `target/release/`.
fn sibling_exe(name: &str) -> anyhow::Result<PathBuf> {
    let self_dir = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("current exe has no parent directory"))?
        .to_owned();
    let path = self_dir.join(format!("{name}.exe"));
    anyhow::ensure!(path.exists(), "sibling executable not found: {}", path.display());
    Ok(path)
}

/// Spawn `live-capture --mode crop` with the given crop rect and wait for it
/// to exit.  The child inherits our stdout so its live-protocol frames flow
/// directly to the downstream pipe (e.g. `live-ws`).
///
/// The child is assigned to `job` so it is automatically killed if this
/// process exits for any reason (crash, Ctrl+C via Task Manager, etc.).
fn spawn_and_wait(
    capture_exe: &PathBuf,
    hwnd: usize,
    crop: &euclid::default::Box2D<u32>,
    args: &CliArgs,
    job: &job_object::JobObject,
) -> anyhow::Result<std::process::ExitStatus> {
    let mut child = Command::new(capture_exe)
        .args([
            "--mode", "crop",
            "--hwnd", &hwnd.to_string(),
            "--crop-min-x", &crop.min.x.to_string(),
            "--crop-min-y", &crop.min.y.to_string(),
            "--crop-max-x", &crop.max.x.to_string(),
            "--crop-max-y", &crop.max.y.to_string(),
            "--fps", &args.fps.to_string(),
            "--stream-id", &args.stream_id,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn live-capture: {e}"))?;

    job.assign(&child)
        .map_err(|e| anyhow::anyhow!("failed to assign child to job object: {e}"))?;

    child.wait().map_err(Into::into)
}

// ── Entry point ─────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    let _ = set_dpi_awareness::per_monitor_v2();
    pretty_env_logger::init();

    let args = CliArgs::parse();
    let poll_interval = Duration::from_secs(args.poll_interval);

    let capture_exe = sibling_exe("live-capture.youtube-music")?;
    log::info!("{LOG_PREFIX} using capture exe: {}", capture_exe.display());

    // Job object auto-kills child processes when this process exits.
    let job = job_object::JobObject::new()
        .map_err(|e| anyhow::anyhow!("failed to create job object: {e}"))?;

    #[expect(clippy::infinite_loop, reason = "This process is designed to run indefinitely, respawning the capture child as needed.")]
    loop {
        // Step 1: find the YTM window.
        let Some(window) = find_ytm_window(&args.title) else {
            log::info!("{LOG_PREFIX} waiting for window \"{}\"\u{2026}", args.title);
            std::thread::sleep(poll_interval);
            continue;
        };

        // Step 2: compute the crop rect.
        let hwnd = HWND(window.hwnd as _);
        let crop = match crop::compute_crop_rect(hwnd) {
            Ok(c) => c,
            Err(e) => {
                log::error!("{LOG_PREFIX} crop computation failed: {e}");
                std::thread::sleep(poll_interval);
                continue;
            }
        };

        log::info!(
            "{LOG_PREFIX} found \"{}\" (hwnd=0x{:X}), crop=({},{})..({},{})",
            window.title, window.hwnd,
            crop.min.x, crop.min.y, crop.max.x, crop.max.y);

        // Step 3: spawn live-capture and wait.
        let started = Instant::now();
        match spawn_and_wait(&capture_exe, window.hwnd, &crop, &args, &job) {
            Ok(status) => log::info!("{LOG_PREFIX} live-capture exited: {status}"),
            Err(e) => log::error!("{LOG_PREFIX} live-capture error: {e}"),
        }

        // Guard against rapid exits (e.g. broken pipe from dead live-ws).
        if started.elapsed() < RAPID_EXIT_THRESHOLD {
            log::warn!(
                "{LOG_PREFIX} live-capture exited too quickly, cooling down for {}s",
                RAPID_EXIT_COOLDOWN.as_secs());
            std::thread::sleep(RAPID_EXIT_COOLDOWN);
        } else {
            std::thread::sleep(poll_interval);
        }
    }
}
