/// Blocking HTTP client for the LiveServer API.
///
/// Thin wrapper around `reqwest::blocking::Client`. One method per endpoint,
/// all return `Result<T, String>` where the error is a human-readable message.
/// Called synchronously from the egui UI thread — a short timeout ensures
/// the UI never hangs for more than ~1s even if the server is unreachable.
use std::time::Duration;

use reqwest::blocking::Client as HttpClient;

use crate::model::*;

/// Connection + response timeout. Keeps the UI responsive even when the server
/// is down — the OS default TCP timeout (20-30s on Windows) would freeze the
/// window completely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(1);

pub struct Client {
    http: HttpClient,
    /// Base URL for the `/streams` API (e.g. `http://localhost:3000/streams`).
    base: String,
}

impl Client {
    pub fn new(server_url: &str) -> Self {
        Self {
            http: HttpClient::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .expect("failed to build HTTP client"),
            base: format!("{server_url}/streams"),
        }
    }

    /// `GET /streams` — list all active capture streams.
    pub fn list_streams(&self) -> Result<Vec<StreamInfo>, String> {
        self.http
            .get(&self.base)
            .send()
            .and_then(|r| r.json())
            .map_err(|e| format!("list streams: {e}"))
    }

    /// `GET /streams/windows` — enumerate capturable windows.
    pub fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        self.http
            .get(format!("{}/windows", self.base))
            .send()
            .and_then(|r| r.json())
            .map_err(|e| format!("list windows: {e}"))
    }

    /// `POST /streams` (resample mode) — create a capture that scales the
    /// full window to `width` x `height`.
    pub fn create_stream_resample(
        &self,
        hwnd: &str,
        width: u32,
        height: u32,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "hwnd": hwnd,
            "width": width,
            "height": height,
        });
        self.http
            .post(&self.base)
            .json(&body)
            .send()
            .and_then(|r| r.json::<CreateStreamResponse>())
            .map(|r| r.id)
            .map_err(|e| format!("create stream (resample): {e}"))
    }

    /// `POST /streams` (crop mode) — create a capture that extracts a subrect
    /// at native resolution.
    ///
    /// `crop_width` / `crop_height` are either a pixel count as a string or `"full"`.
    #[expect(clippy::redundant_closure_for_method_calls, reason = "generated code")]
    pub fn create_stream_crop(
        &self,
        hwnd: &str,
        crop_width: &str,
        crop_height: &str,
        crop_align: &str,
    ) -> Result<String, String> {
        // The server accepts either a number or the string "full" for crop dimensions.
        let w: serde_json::Value = crop_width
            .parse::<u32>()
            .map_or_else(|_| serde_json::json!("full"), |n| serde_json::json!(n));
        let h: serde_json::Value = crop_height
            .parse::<u32>()
            .map_or_else(|_| serde_json::json!("full"), |n| serde_json::json!(n));

        let body = serde_json::json!({
            "hwnd": hwnd,
            "cropWidth": w,
            "cropHeight": h,
            "cropAlign": crop_align,
        });
        self.http
            .post(&self.base)
            .json(&body)
            .send()
            .and_then(|r| r.json::<CreateStreamResponse>())
            .map(|r| r.id)
            .map_err(|e| format!("create stream (crop): {e}"))
    }

    /// `DELETE /streams/:id` — destroy a capture stream.
    pub fn destroy_stream(&self, id: &str) -> Result<(), String> {
        self.http
            .delete(format!("{}/{id}", self.base))
            .send()
            .map(|_| ())
            .map_err(|e| format!("destroy stream: {e}"))
    }

    /// `GET /streams/auto` — get auto-selector status.
    pub fn get_auto_status(&self) -> Result<AutoStatus, String> {
        self.http
            .get(format!("{}/auto", self.base))
            .send()
            .and_then(|r| r.json())
            .map_err(|e| format!("auto status: {e}"))
    }

    /// `POST /streams/auto` — start the auto-selector.
    pub fn start_auto(&self) -> Result<AutoStatus, String> {
        self.http
            .post(format!("{}/auto", self.base))
            .send()
            .and_then(|r| r.json())
            .map_err(|e| format!("start auto: {e}"))
    }

    /// `DELETE /streams/auto` — stop the auto-selector.
    pub fn stop_auto(&self) -> Result<(), String> {
        self.http
            .delete(format!("{}/auto", self.base))
            .send()
            .map(|_| ())
            .map_err(|e| format!("stop auto: {e}"))
    }
}
