//! KPM (keystrokes per minute) WebSocket relay.
//!
//! - **Input**:  `WS /api/v1/kpm/ws/input` — receives binary `live-protocol`
//!   `KpmUpdate` messages from `live-kpm` via `live-ws`.
//! - **Output**: `WS /api/v1/kpm/ws` — pushes `{"kpm": N}` JSON text to all
//!   connected frontend clients.  Sends the cached value on connect.
//!
//! The input side parses the 8-byte frame header + i64 LE payload.
//! The output side converts to JSON and broadcasts via a `watch` channel.

use crate::state::AppState;

use live_protocol::{HEADER_SIZE, MessageType};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use tokio::sync::watch;

use std::sync::Arc;

// ── State ───────────────────────────────────────────────────────────────

/// Global KPM relay state.
pub struct KpmState {
    /// Current KPM value.  `None` = no encoder connected.
    /// Watch channel: single writer (input WS), many readers (viewer WS).
    tx: watch::Sender<Option<i64>>,
    rx: watch::Receiver<Option<i64>>,
}

impl KpmState {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(None);
        Self { tx, rx }
    }
}

// ── Routes ──────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/kpm/ws/input", get(kpm_input))
        .route("/api/v1/kpm/ws", get(kpm_viewer))
}

// ── Input WS ────────────────────────────────────────────────────────────

/// `WS /api/v1/kpm/ws/input` — KPM encoder input from `live-ws`.
async fn kpm_input(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_kpm_input(socket, state))
}

/// Parse binary `KpmUpdate` messages (i64 LE at header offset 8) and
/// publish via the watch channel.
async fn handle_kpm_input(mut socket: WebSocket, state: Arc<AppState>) {
    log::info!("[kpm] encoder connected");

    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Binary(data) = msg else { continue };
        if data.len() < HEADER_SIZE + 8 { continue; }

        let msg_type = data[0];
        if msg_type != MessageType::KpmUpdate as u8 { continue; }

        // KpmUpdate payload: i64 LE at offset HEADER_SIZE.
        let kpm = i64::from_le_bytes(
            data[HEADER_SIZE..HEADER_SIZE + 8].try_into().unwrap());

        // Publish to all viewers via watch channel.
        let _ = state.kpm.tx.send(Some(kpm));
    }

    // Encoder disconnected — signal null to viewers.
    let _ = state.kpm.tx.send(None);
    log::info!("[kpm] encoder disconnected");
}

// ── Viewer WS ───────────────────────────────────────────────────────────

/// `WS /api/v1/kpm/ws` — frontend KPM display.
async fn kpm_viewer(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_kpm_viewer(socket, state))
}

/// Push `{"kpm": N}` or `{"kpm": null}` JSON text on every value change.
/// Sends the current value immediately on connect.
async fn handle_kpm_viewer(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.kpm.rx.clone();

    log::info!("[kpm] viewer connected");

    // Send current value immediately.
    let initial = *rx.borrow_and_update();
    let json = kpm_json(initial);
    if socket.send(Message::Text(json.into())).await.is_err() {
        return;
    }

    // Push on every change.
    while rx.changed().await.is_ok() {
        let value = *rx.borrow_and_update();
        let json = kpm_json(value);
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }

    log::info!("[kpm] viewer disconnected");
}

/// Format a KPM value as JSON.
fn kpm_json(value: Option<i64>) -> String {
    match value {
        Some(n) => format!(r#"{{"kpm":{n}}}"#),
        None => r#"{"kpm":null}"#.to_owned(),
    }
}
