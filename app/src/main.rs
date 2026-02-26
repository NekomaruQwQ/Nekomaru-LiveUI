//! Minimal wry webview host for Nekomaru LiveUI.
//!
//! Opens a non-resizable window at the stream resolution and loads the
//! LiveServer frontend. This is a thin shell — all capture, encoding, and
//! stream management lives in `live-capture.exe` and LiveServer.

/// Reads `LIVE_PORT` from the environment, panics if not set or invalid,
/// and constructs the server URL.
fn get_server_url() -> String {
    let port = std::env::var("LIVE_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .expect("LIVE_PORT not set or is not a valid port number");
    format!("http://localhost:{port}")
}

fn main() {
    live_app::run_webview("Nekomaru LiveUI v2", &get_server_url());
}
