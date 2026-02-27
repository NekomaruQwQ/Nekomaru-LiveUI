/// Data types mirroring the LiveServer HTTP API responses.
///
/// These are deserialized from JSON returned by the `/streams` endpoints.
/// Field names use `#[serde(rename)]` where the server uses camelCase.
use serde::{Deserialize, Serialize};

/// A capture stream as returned by `GET /streams`.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamInfo {
    pub id: String,
    /// Hex-formatted window handle (e.g. `"0x1A2B3C"`).
    pub hwnd: String,
    /// One of `"starting"`, `"running"`, `"stopped"`.
    pub status: String,
    /// Monotonic counter, bumped each time the underlying capture is replaced.
    pub generation: u32,
}

/// A capturable window as returned by `GET /streams/windows`.
#[derive(Debug, Clone, Deserialize)]
#[expect(dead_code, reason = "deserialization struct; pid is unused but must be present for JSON parsing")]
pub struct WindowInfo {
    /// Raw window handle (numeric). Format as `0x{:X}` before sending to the server.
    pub hwnd: usize,
    pub pid: u32,
    pub title: String,
    pub executable_path: String,
    /// Client-area width in pixels.
    pub width: u32,
    /// Client-area height in pixels.
    pub height: u32,
}

/// Auto-selector status as returned by `GET /streams/auto`.
#[derive(Debug, Clone, Deserialize)]
pub struct AutoStatus {
    pub active: bool,
    #[expect(dead_code, reason = "deserialization field; present in JSON but not read by app logic")]
    #[serde(rename = "currentStreamId")]
    pub current_stream_id: Option<String>,
    #[serde(rename = "currentHwnd")]
    pub current_hwnd: Option<String>,
}

/// Auto-selector include/exclude pattern lists as returned by
/// `GET /streams/auto/config`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SelectorConfig {
    #[serde(rename = "includeList")]
    pub include_list: Vec<String>,
    #[serde(rename = "excludeList")]
    pub exclude_list: Vec<String>,
}
