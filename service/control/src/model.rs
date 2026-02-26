/// Data types mirroring the LiveServer HTTP API responses.
///
/// These are deserialized from JSON returned by the `/streams` endpoints.
/// Field names use `#[serde(rename)]` where the server uses camelCase.
use serde::Deserialize;

/// A capture stream as returned by `GET /streams`.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamInfo {
    pub id: String,
    /// Hex-formatted window handle (e.g. `"0x1A2B3C"`).
    pub hwnd: String,
    /// One of `"starting"`, `"running"`, `"stopped"`.
    pub status: String,
}

/// A capturable window as returned by `GET /streams/windows`.
#[derive(Debug, Clone, Deserialize)]
pub struct WindowInfo {
    /// Raw window handle (numeric). Format as `0x{:X}` before sending to the server.
    pub hwnd: usize,
    pub pid: u32,
    pub title: String,
    pub executable_path: String,
}

/// Auto-selector status as returned by `GET /streams/auto`.
#[derive(Debug, Clone, Deserialize)]
pub struct AutoStatus {
    pub active: bool,
    #[serde(rename = "currentStreamId")]
    pub current_stream_id: Option<String>,
    #[serde(rename = "currentHwnd")]
    pub current_hwnd: Option<String>,
}

/// Response from `POST /streams` — contains the new stream's ID.
#[derive(Debug, Deserialize)]
pub struct CreateStreamResponse {
    pub id: String,
}
