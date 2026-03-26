//! Audio WebSocket relay.
//!
//! - **Encoder input**: `WS /internal/audio` — receives binary
//!   `live-protocol` frames from `live-ws` and broadcasts to all viewers.
//! - **Frontend viewer**: `WS /api/audio` — pushes relayed frames.
//!   On connect, sends cached AudioConfig for immediate worklet setup.
//!
//! The server does NOT buffer audio — it relays frames.  It caches only
//! the last AudioConfig message (for late-joiners).  Unlike video, there
//! is no keyframe concept — all PCM chunks are independently playable.

use crate::state::AppState;

use live_protocol::{HEADER_SIZE, MessageType};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use tokio::sync::broadcast;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// ── Broadcast channel capacity ──────────────────────────────────────────

/// Broadcast capacity.  At 100 chunks/sec (10ms chunks), 200 frames is
/// ~2 seconds of buffer.  Slow viewers receive `Lagged` errors and skip
/// ahead — audio recovers instantly from gaps (unlike video).
const BROADCAST_CAPACITY: usize = 200;

// ── State ───────────────────────────────────────────────────────────────

/// Global audio relay state (single instance, not per-stream-ID).
pub struct AudioState {
    /// Broadcast channel for fan-out to all frontend viewers.
    tx: broadcast::Sender<Vec<u8>>,
    /// Cached raw AudioConfig message (for late-joiners).
    cached_config: Mutex<Option<Vec<u8>>>,
    /// Whether an encoder WS is currently connected.
    encoder_connected: AtomicBool,
}

impl AudioState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            tx,
            cached_config: Mutex::new(None),
            encoder_connected: AtomicBool::new(false),
        }
    }
}

// ── Routes ──────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/internal/audio", get(encoder_input))
        .route("/api/audio", get(viewer_ws))
}

// ── Encoder Input WS ────────────────────────────────────────────────────

/// `WS /internal/audio` — encoder input from `live-ws`.
async fn encoder_input(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_encoder(socket, state))
}

/// Handle an audio encoder WS connection.  Receives binary `live-protocol`
/// frames, caches AudioConfig, and broadcasts to all viewers.
///
/// Also updates the `$microphone` computed string so the frontend can show
/// audio connection status without a separate polling process.
async fn handle_encoder(mut socket: WebSocket, state: Arc<AppState>) {
    state.audio.encoder_connected.store(true, Ordering::Relaxed);
    state.strings.write().await.set_computed("$microphone", "on".to_owned());
    log::info!("audio encoder connected");

    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Binary(data) = msg else { continue };
        if data.len() < HEADER_SIZE { continue; }
        let msg_type = data.first().copied().unwrap();

        // Cache AudioConfig for late-joiners.
        if msg_type == MessageType::AudioConfig as u8 {
            *state.audio.cached_config.lock().unwrap() = Some(data.to_vec());
            log::info!("audio cached AudioConfig ({}B)", data.len());
        }

        // Fan-out to all subscribed viewers.
        let _ = state.audio.tx.send(data.to_vec());
    }

    state.audio.encoder_connected.store(false, Ordering::Relaxed);
    state.strings.write().await.clear_computed("$microphone");
    log::info!("audio encoder disconnected");
}

// ── Frontend Viewer WS ──────────────────────────────────────────────────

/// `WS /api/audio` — frontend audio viewer.
async fn viewer_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_viewer(socket, state))
}

/// Handle a frontend viewer WS connection.  Sends cached AudioConfig on
/// connect (for immediate worklet setup), then relays broadcast messages.
async fn handle_viewer(mut socket: WebSocket, state: Arc<AppState>) {
    // Subscribe BEFORE sending cached config, so we don't miss frames
    // sent between cache-send and subscribe.
    let mut rx = state.audio.tx.subscribe();

    // Late-joiner: send cached AudioConfig.
    let cached_config = state.audio.cached_config.lock().unwrap().clone();
    if let Some(config) = cached_config
        && socket.send(Message::Binary(config.into())).await.is_err()
    {
        return;
    }

    log::info!("audio viewer connected");

    // Relay loop: forward broadcast messages to this viewer.
    loop {
        match rx.recv().await {
            Ok(data) => {
                if socket.send(Message::Binary(data.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                log::debug!("audio viewer lagged {n} messages");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    log::info!("audio viewer disconnected");
}
