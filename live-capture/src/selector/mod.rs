//! Auto-selector: foreground window polling + pattern matching.
//!
//! Runs on a dedicated thread, polls the foreground window every 2 seconds,
//! matches against patterns from the server config, and sends swap commands
//! to the capture loop via a channel.

pub mod config;

use config::{PresetConfig, should_capture};

use std::sync::mpsc;
use std::time::Duration;

/// Command sent from the selector thread to the capture loop.
pub struct SwapCommand {
    /// New window handle to capture.
    pub hwnd: isize,
    /// Human-readable label for the captured window.
    pub capture_info: String,
    /// Mode tag from the matched pattern (e.g. "code", "game").
    pub mode: Option<String>,
}

/// Configuration for the selector thread.
pub struct SelectorConfig {
    /// URL to poll for the selector preset config (GET, returns JSON).
    pub config_url: String,
    /// URL to POST stream info on capture switch.
    pub event_url: String,
    /// Poll interval for foreground window checks.
    pub poll_interval: Duration,
}

/// Spawn the selector polling thread.  Returns the receiving end of the
/// swap command channel.
pub fn spawn_selector(config: SelectorConfig) -> mpsc::Receiver<SwapCommand> {
    let (tx, rx) = mpsc::channel();

    std::thread::Builder::new()
        .name("selector".into())
        .spawn(move || selector_loop(tx, config))
        .expect("failed to spawn selector thread");

    rx
}

/// Main selector loop.  Polls the foreground window, matches patterns,
/// sends swap commands, and POSTs metadata to the server.
fn selector_loop(tx: mpsc::Sender<SwapCommand>, config: SelectorConfig) {
    log::info!("selector started (poll: {:?}, config: {})",
        config.poll_interval, config.config_url);

    let mut last_hwnd: Option<isize> = None;
    let mut preset_config: Option<PresetConfig> = None;
    let mut config_poll_counter: u32 = 0;

    // Poll config on first iteration, then every ~10 iterations (20s at 2s poll).
    const CONFIG_POLL_EVERY: u32 = 10;

    loop {
        std::thread::sleep(config.poll_interval);

        // Periodically refresh the preset config from the server.
        if config_poll_counter % CONFIG_POLL_EVERY == 0 {
            match fetch_config(&config.config_url) {
                Ok(cfg) => {
                    log::debug!("fetched selector config: preset=\"{}\"", cfg.preset);
                    preset_config = Some(cfg);
                }
                Err(e) => {
                    log::warn!("failed to fetch selector config: {e}");
                    // Keep using the last known config.
                }
            }
        }
        config_poll_counter = config_poll_counter.wrapping_add(1);

        let Some(ref cfg) = preset_config else { continue };
        let Some(patterns) = cfg.active_patterns() else { continue };

        // Get the current foreground window.
        let Some(info) = enumerate_windows::get_foreground_window() else { continue };

        // Skip if foreground hasn't changed.
        let hwnd = info.hwnd as isize;
        if last_hwnd == Some(hwnd) { continue; }

        // Match against patterns.
        let exe_path = info.executable_path.to_string_lossy().to_string();
        let Some(capture_match) = should_capture(patterns, &exe_path, &info.title) else {
            continue;
        };

        // Resolve capture info: prefer exe FileDescription, fall back to window title.
        let capture_info = win32_version_info::VersionInfo::from_file(&info.executable_path)
            .ok()
            .map(|v| v.file_description)
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| info.title.clone());

        log::info!("switching to HWND 0x{:X} ({})", hwnd, capture_info);

        // Send swap command to the capture loop.
        let cmd = SwapCommand {
            hwnd,
            capture_info: capture_info.clone(),
            mode: capture_match.mode.clone(),
        };
        if tx.send(cmd).is_err() {
            log::info!("capture loop closed, selector exiting");
            break;
        }

        last_hwnd = Some(hwnd);

        // POST stream info to the server (best-effort, non-blocking).
        post_stream_info(&config.event_url, hwnd, &info.title, &capture_info, &capture_match.mode);
    }
}

/// Fetch the selector preset config from the server.
fn fetch_config(url: &str) -> anyhow::Result<PresetConfig> {
    let body: String = ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("HTTP GET failed: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;
    let config: PresetConfig = serde_json::from_str(&body)?;
    Ok(config)
}

/// POST stream info to the server on capture switch (best-effort).
fn post_stream_info(
    url: &str,
    hwnd: isize,
    title: &str,
    file_description: &str,
    mode: &Option<String>,
) {
    let body = serde_json::json!({
        "hwnd": format!("0x{hwnd:X}"),
        "title": title,
        "file_description": file_description,
        "mode": mode,
    });

    if let Err(e) = ureq::post(url)
        .header("Content-Type", "application/json")
        .send(body.to_string().as_bytes()) {
        log::warn!("failed to POST stream info: {e}");
    }
}
