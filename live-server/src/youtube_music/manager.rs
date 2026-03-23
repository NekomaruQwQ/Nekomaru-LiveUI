//! YouTube Music stream manager.
//!
//! Polls `enumerate_windows::enumerate_windows()` every 5 seconds looking for
//! a window titled "YouTube Music - Nekomaru LiveUI".  When found, creates
//! (or replaces) a crop-mode stream capturing the bottom playback bar.  When
//! the window disappears, the stream is destroyed.

use crate::constant::{STREAM_ID_YTM, YTM_POLL_INTERVAL_MS, YOUTUBE_MUSIC_WINDOW_TITLE};
use crate::video::process::StreamRegistry;

use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;

// ── YTM State ────────────────────────────────────────────────────────────────

pub struct YtmState {
    pub active: bool,
    pub last_known_hwnd: Option<String>,
    pub poll_handle: Option<JoinHandle<()>>,
}

impl YtmState {
    pub const fn new() -> Self {
        Self { active: false, last_known_hwnd: None, poll_handle: None }
    }

    pub fn start(
        &mut self,
        ytm_arc: &Arc<RwLock<Self>>,
        streams_arc: &Arc<RwLock<StreamRegistry>>,
    ) {
        if self.active { return; }
        self.active = true;

        let ytm = Arc::clone(ytm_arc);
        let streams = Arc::clone(streams_arc);

        self.poll_handle = Some(tokio::spawn(async move {
            // Immediate first poll.
            poll_once(&ytm, &streams).await;

            let mut interval = tokio::time::interval(
                std::time::Duration::from_millis(YTM_POLL_INTERVAL_MS));

            loop {
                interval.tick().await;
                poll_once(&ytm, &streams).await;
            }
        }));

        log::info!("started");
    }

    /// Stop polling and destroy the managed YouTube Music stream.
    pub async fn stop(&mut self, streams_arc: &Arc<RwLock<StreamRegistry>>) {
        if !self.active { return; }

        if let Some(handle) = self.poll_handle.take() {
            handle.abort();
        }

        self.active = false;
        self.last_known_hwnd = None;

        streams_arc.write().await.destroy_stream(STREAM_ID_YTM);

        log::info!("stopped");
    }
}

impl Drop for YtmState {
    fn drop(&mut self) {
        if let Some(handle) = self.poll_handle.take() {
            handle.abort();
        }
    }
}

// ── Poll Logic ───────────────────────────────────────────────────────────────

async fn poll_once(
    ytm_arc: &Arc<RwLock<YtmState>>,
    streams_arc: &Arc<RwLock<StreamRegistry>>,
) {
    let windows = tokio::task::spawn_blocking(enumerate_windows::enumerate_windows)
        .await
        .unwrap_or_default();

    let ytm = windows.iter().find(|w| w.title == YOUTUBE_MUSIC_WINDOW_TITLE);

    if let Some(ytm) = ytm {
        let hwnd_str = format!("0x{:X}", ytm.hwnd);

        {
            let state = ytm_arc.read().await;
            if state.last_known_hwnd.as_deref() == Some(&hwnd_str) { return; }
        }

        log::info!("window detected: {hwnd_str} ({}x{})", ytm.width, ytm.height);

        let Some(crop) = crate::constant::get_youtube_music_crop_geometry(ytm.width, ytm.height)
            else { return; };

        {
            let mut streams = streams_arc.write().await;
            streams.replace_crop_stream(
                STREAM_ID_YTM, &hwnd_str, &crop, Some(2), streams_arc);
        }

        ytm_arc.write().await.last_known_hwnd = Some(hwnd_str);
    } else {
        let has_hwnd = ytm_arc.read().await.last_known_hwnd.is_some();
        if has_hwnd {
            streams_arc.write().await.destroy_stream(STREAM_ID_YTM);
            ytm_arc.write().await.last_known_hwnd = None;
            log::info!("window disappeared, stream destroyed");
        }
    }
}
