//! Video WebSocket relay.
//!
//! - **Encoder input**: `WS /internal/streams/:id` — receives binary
//!   `live-protocol` frames from `live-ws` and broadcasts to all viewers.
//! - **Frontend viewer**: `WS /api/streams/:id` — pushes relayed frames.
//!   On connect, sends cached CodecParams + keyframe for immediate playback.
//! - **Init**: `GET /api/streams/:id/init` — pre-built decoder config
//!   (`avc1.*` codec string + avcC descriptor) from cached CodecParams.
//! - **List**: `GET /api/streams` — active stream IDs.
//!
//! The server does NOT buffer frames — it relays them.  It caches:
//! - Last CodecParams message per stream (for `/init` and late-joiners)
//! - Last keyframe message per stream (for late-joiners)

use crate::state::AppState;

use live_protocol::{HEADER_SIZE, MessageType, flags};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use base64::Engine as _;
use tokio::sync::broadcast;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// ── Broadcast channel capacity ──────────────────────────────────────────

/// Broadcast capacity.  At 60fps, 32 frames is ~500ms of buffer.
/// Slow viewers receive `Lagged` errors and skip ahead.
const BROADCAST_CAPACITY: usize = 32;

// ── State ───────────────────────────────────────────────────────────────

/// Per-stream relay state.
struct StreamSlot {
    /// Broadcast channel for fan-out to all frontend viewers.
    tx: broadcast::Sender<Vec<u8>>,
    /// Cached raw CodecParams message (for `/init` and late-joiners).
    cached_codec_params: Mutex<Option<Vec<u8>>>,
    /// Cached raw keyframe message (for late-joiners).
    cached_keyframe: Mutex<Option<Vec<u8>>>,
    /// Whether an encoder WS is currently connected.
    encoder_connected: AtomicBool,
}

impl StreamSlot {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            tx,
            cached_codec_params: Mutex::new(None),
            cached_keyframe: Mutex::new(None),
            encoder_connected: AtomicBool::new(false),
        }
    }
}

/// Global video relay state.  Holds all stream slots.
pub struct VideoState {
    streams: Mutex<HashMap<String, Arc<StreamSlot>>>,
}

impl VideoState {
    pub fn new() -> Self {
        Self { streams: Mutex::new(HashMap::new()) }
    }

    /// Get or create the slot for a stream ID.
    fn get_or_create(&self, id: &str) -> Arc<StreamSlot> {
        let mut map = self.streams.lock().unwrap();
        Arc::clone(
            map.entry(id.to_owned())
                .or_insert_with(|| Arc::new(StreamSlot::new())))
    }

    /// List stream IDs that have an encoder connected.
    fn list_active(&self) -> Vec<String> {
        let map = self.streams.lock().unwrap();
        map.iter()
            .filter(|&(_, slot)| slot.encoder_connected.load(Ordering::Relaxed))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get a stream slot if it exists.
    fn get(&self, id: &str) -> Option<Arc<StreamSlot>> {
        let map = self.streams.lock().unwrap();
        map.get(id).cloned()
    }
}

// ── Routes ──────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/streams", get(list_streams))
        .route("/api/streams/{id}/init", get(get_init))
        .route("/internal/streams/{id}", get(encoder_input))
        .route("/api/streams/{id}", get(viewer_ws))
}

/// `GET /api/streams` — list active stream IDs.
async fn list_streams(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ids: Vec<_> = state.video.list_active()
        .into_iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect();
    Json(serde_json::Value::Array(ids))
}

/// `GET /api/streams/:id/init` — pre-built decoder configuration.
///
/// Parses cached CodecParams via `live-protocol` to build the `avc1.PPCCLL`
/// codec string and ISO 14496-15 avcC descriptor.  The frontend passes
/// these to `VideoDecoder.configure()`.
async fn get_init(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Some(slot) = state.video.get(&id) else {
        return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "stream not found" }))).into_response();
    };

    if !slot.encoder_connected.load(Ordering::Relaxed) {
        return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "stream not found" }))).into_response();
    }

    let raw = {
        let guard = slot.cached_codec_params.lock().unwrap();
        match guard.clone() {
            Some(v) => v,
            None => return (StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "codec params not yet available" }))).into_response(),
        }
    };

    // Parse the CodecParams payload (skip the 8-byte frame header).
    let payload = &raw[HEADER_SIZE..];
    let params = match live_protocol::video::read_codec_params_payload(payload) {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("corrupt codec params: {e}") }))).into_response(),
    };

    let codec = live_protocol::avcc::build_codec_string(&params.sps);
    let desc = live_protocol::avcc::build_avcc_descriptor(&params.sps, &params.pps);

    let desc_b64 = base64::engine::general_purpose::STANDARD.encode(&desc);

    Json(serde_json::json!({
        "codec": codec,
        "width": params.width,
        "height": params.height,
        "description": desc_b64,
    })).into_response()
}

// ── Encoder Input WS ────────────────────────────────────────────────────

/// `WS /internal/streams/:id` — encoder input from `live-ws`.
async fn encoder_input(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_encoder(socket, id, state))
}

/// Handle an encoder WS connection.  Receives binary `live-protocol`
/// frames, caches CodecParams + keyframes, and broadcasts to all viewers.
async fn handle_encoder(mut socket: WebSocket, id: String, state: Arc<AppState>) {
    let slot = state.video.get_or_create(&id);
    slot.encoder_connected.store(true, Ordering::Relaxed);
    log::info!("@{id} encoder connected");

    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Binary(data) = msg else { continue };
        if data.len() < HEADER_SIZE { continue; }
        let msg_type = data.first().copied().unwrap();
        let msg_flags = data.get(1).copied().unwrap();

        // Cache CodecParams and keyframes for late-joiners and /init.
        match msg_type {
            t if t == MessageType::CodecParams as u8 => {
                *slot.cached_codec_params.lock().unwrap() = Some(data.to_vec());
                log::info!("@{id} cached CodecParams ({}B)", data.len());
            }
            t if t == MessageType::Frame as u8
                && (msg_flags & flags::IS_KEYFRAME) != 0 =>
            {
                *slot.cached_keyframe.lock().unwrap() = Some(data.to_vec());
            }
            _ => {}
        }

        // Fan-out to all subscribed viewers.  Ignoring send errors —
        // they just mean no viewers are connected.
        let _ = slot.tx.send(data.to_vec());
    }

    slot.encoder_connected.store(false, Ordering::Relaxed);
    log::info!("@{id} encoder disconnected");
}

// ── Frontend Viewer WS ──────────────────────────────────────────────────

/// `WS /api/streams/:id` — frontend viewer.
async fn viewer_ws(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_viewer(socket, id, state))
}

/// Handle a frontend viewer WS connection.  Sends cached CodecParams +
/// keyframe on connect (for immediate playback), then relays broadcast
/// messages.
async fn handle_viewer(mut socket: WebSocket, id: String, state: Arc<AppState>) {
    let slot = state.video.get_or_create(&id);

    // Subscribe to the broadcast channel BEFORE sending cached messages,
    // so we don't miss frames sent between cache-send and subscribe.
    let mut rx = slot.tx.subscribe();

    // Late-joiner: send cached CodecParams + keyframe for immediate decode.
    // Clone out of the Mutex before awaiting to avoid holding the guard
    // across an await point (MutexGuard is !Send).
    let cached_params = slot.cached_codec_params.lock().unwrap().clone();
    let cached_keyframe = slot.cached_keyframe.lock().unwrap().clone();

    if let Some(params) = cached_params
        && socket.send(Message::Binary(params.into())).await.is_err()
    {
        return;
    }
    if let Some(keyframe) = cached_keyframe
        && socket.send(Message::Binary(keyframe.into())).await.is_err()
    {
        return;
    }

    log::info!("@{id} viewer connected");

    // Relay loop: forward broadcast messages to this viewer.
    loop {
        match rx.recv().await {
            Ok(data) => {
                if socket.send(Message::Binary(data.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                log::debug!("@{id} viewer lagged {n} messages");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    log::info!("@{id} viewer disconnected");
}
