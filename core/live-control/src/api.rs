/// Blocking HTTP client for the LiveServer API.
///
/// Thin wrapper around `reqwest::blocking::Client`. One method per endpoint,
/// all return `Result<T, String>` where the error is a human-readable message.
/// Called synchronously from the egui UI thread — a short timeout ensures
/// the UI never hangs for more than ~1s even if the server is unreachable.
use std::collections::HashMap;
use std::time::Duration;

use reqwest::blocking::Client as HttpClient;

use crate::data::*;

/// Connection + response timeout. Keeps the UI responsive even when the server
/// is down — the OS default TCP timeout (20-30s on Windows) would freeze the
/// window completely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(1);

pub struct Client {
    http: HttpClient,
    /// Base URL for the server (e.g. `http://localhost:3000`).
    server_url: String,
}

impl Client {
    pub fn new(server_url: &str) -> Self {
        Self {
            http: HttpClient::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .expect("failed to build HTTP client"),
            server_url: server_url.to_owned(),
        }
    }

    // ── Streams ──────────────────────────────────────────────────────────

    /// `GET /streams` — list all active capture streams.
    pub fn list_streams(&self) -> Result<Vec<StreamInfo>, String> {
        self.http
            .get(format!("{}/streams", self.server_url))
            .send()
            .and_then(reqwest::blocking::Response::json)
            .map_err(|e| format!("list streams: {e}"))
    }

    /// `GET /streams/windows` — enumerate capturable windows.
    pub fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        self.http
            .get(format!("{}/streams/windows", self.server_url))
            .send()
            .and_then(reqwest::blocking::Response::json)
            .map_err(|e| format!("list windows: {e}"))
    }

    /// `DELETE /streams/:id` — destroy a capture stream.
    #[expect(dead_code, reason = "API method — will be wired to a UI button")]
    pub fn destroy_stream(&self, id: &str) -> Result<(), String> {
        self.http
            .delete(format!("{}/streams/{id}", self.server_url))
            .send()
            .map(|_| ())
            .map_err(|e| format!("destroy stream: {e}"))
    }

    // ── Auto-selector ────────────────────────────────────────────────────

    /// `GET /streams/auto` — get auto-selector status.
    pub fn get_auto_status(&self) -> Result<AutoStatus, String> {
        self.http
            .get(format!("{}/streams/auto", self.server_url))
            .send()
            .and_then(reqwest::blocking::Response::json)
            .map_err(|e| format!("auto status: {e}"))
    }

    /// `POST /streams/auto` — start the auto-selector.
    pub fn start_auto(&self) -> Result<AutoStatus, String> {
        self.http
            .post(format!("{}/streams/auto", self.server_url))
            .send()
            .and_then(reqwest::blocking::Response::json)
            .map_err(|e| format!("start auto: {e}"))
    }

    /// `DELETE /streams/auto` — stop the auto-selector.
    pub fn stop_auto(&self) -> Result<(), String> {
        self.http
            .delete(format!("{}/streams/auto", self.server_url))
            .send()
            .map(|_| ())
            .map_err(|e| format!("stop auto: {e}"))
    }

    /// `GET /streams/auto/config` — get the include/exclude pattern lists.
    pub fn get_auto_config(&self) -> Result<SelectorConfig, String> {
        self.http
            .get(format!("{}/streams/auto/config", self.server_url))
            .send()
            .and_then(reqwest::blocking::Response::json)
            .map_err(|e| format!("auto config: {e}"))
    }

    /// `PUT /streams/auto/config` — replace the include/exclude pattern lists.
    pub fn set_auto_config(&self, config: &SelectorConfig) -> Result<(), String> {
        self.http
            .put(format!("{}/streams/auto/config", self.server_url))
            .json(config)
            .send()
            .map(|_| ())
            .map_err(|e| format!("set auto config: {e}"))
    }

    // ── String store ─────────────────────────────────────────────────────

    /// `GET /strings` — get all key-value pairs.
    pub fn get_strings(&self) -> Result<HashMap<String, String>, String> {
        self.http
            .get(format!("{}/strings", self.server_url))
            .send()
            .and_then(reqwest::blocking::Response::json)
            .map_err(|e| format!("get strings: {e}"))
    }

    /// `PUT /strings/:key` — set a string value.
    pub fn set_string(&self, key: &str, value: &str) -> Result<(), String> {
        self.http
            .put(format!("{}/strings/{key}", self.server_url))
            .json(&serde_json::json!({ "value": value }))
            .send()
            .map(|_| ())
            .map_err(|e| format!("set string: {e}"))
    }

    /// `DELETE /strings/:key` — delete a string.
    pub fn delete_string(&self, key: &str) -> Result<(), String> {
        self.http
            .delete(format!("{}/strings/{key}", self.server_url))
            .send()
            .map(|_| ())
            .map_err(|e| format!("delete string: {e}"))
    }
}
