//! Auto-selector: foreground window polling + pattern matching.
//!
//! Runs on a dedicated thread, polls the foreground window every 2 seconds,
//! matches against patterns from the server config, and sends swap commands
//! to the capture loop via a channel.  POSTs the current capture info to
//! the server on every tick (heartbeat) so computed strings stay fresh even
//! after a server restart.

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
}

/// Configuration for the selector thread.
pub struct SelectorConfig {
    /// URL to poll for the selector preset config (GET, returns JSON).
    pub config_url: String,
    /// URL to POST stream info on every poll tick.
    pub info_url: String,
    /// Poll interval for foreground window checks.
    pub poll_interval: Duration,
}

/// Cached stream info from the last successful pattern match.
/// Re-posted on every tick to keep the server's computed strings fresh.
struct CachedStreamInfo {
    hwnd: isize,
    title: String,
    file_description: String,
    mode: Option<String>,
}

/// Spawn the selector polling thread.  Returns the receiving end of the
/// swap command channel.
pub fn spawn_selector(config: SelectorConfig) -> mpsc::Receiver<SwapCommand> {
    let (tx, rx) = mpsc::channel();

    std::thread::Builder::new()
        .name("selector".into())
        .spawn(move || selector_loop(&tx, &config))
        .expect("failed to spawn selector thread");

    rx
}

/// Main selector loop.  Polls the foreground window, matches patterns,
/// sends swap commands, and POSTs capture info to the server every tick.
fn selector_loop(tx: &mpsc::Sender<SwapCommand>, config: &SelectorConfig) {
    // Poll config on first iteration, then every ~10 iterations (20s at 2s poll).
    const CONFIG_POLL_EVERY: u32 = 10;

    log::info!("selector started (poll: {:?}, config: {})",
        config.poll_interval, config.config_url);

    let mut last_info: Option<CachedStreamInfo> = None;
    let mut preset_config: Option<PresetConfig> = None;
    let mut config_poll_counter: u32 = 0;

    loop {
        std::thread::sleep(config.poll_interval);

        // Periodically refresh the preset config from the server.
        if config_poll_counter.is_multiple_of(CONFIG_POLL_EVERY) {
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

        // Check if the foreground window changed since the last match.
        let hwnd = info.hwnd as isize;
        let hwnd_changed = last_info.as_ref().map_or(true, |li| li.hwnd != hwnd);

        if hwnd_changed {
            // Match against patterns.
            let exe_path = info.executable_path.to_string_lossy().to_string();
            if let Some(capture_match) = should_capture(patterns, &exe_path, &info.title) {
                // Resolve capture info: prefer exe FileDescription, fall back to window title.
                let file_description = win32_version_info::VersionInfo::from_file(&info.executable_path)
                    .ok()
                    .map(|v| v.file_description)
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| info.title.clone());

                log::info!("switching to HWND 0x{hwnd:X} ({file_description})");

                // Send swap command to the capture loop.
                let cmd = SwapCommand {
                    hwnd,
                    capture_info: file_description.clone(),
                };
                if tx.send(cmd).is_err() {
                    log::info!("capture loop closed, selector exiting");
                    break;
                }

                last_info = Some(CachedStreamInfo {
                    hwnd,
                    title: info.title.clone(),
                    file_description,
                    mode: capture_match.mode,
                });
            }
            // If the new window doesn't match, keep last_info unchanged —
            // capture is still running on the previously matched window.
        }

        // POST current capture info to the server on every tick (heartbeat).
        if let Some(ref cached) = last_info {
            post_stream_info(
                &config.info_url,
                cached.hwnd,
                &cached.title,
                &cached.file_description,
                cached.mode.as_ref());
        }
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

/// POST stream info to the server (best-effort, called every poll tick).
fn post_stream_info(
    url: &str,
    hwnd: isize,
    title: &str,
    file_description: &str,
    mode: Option<&String>,
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
