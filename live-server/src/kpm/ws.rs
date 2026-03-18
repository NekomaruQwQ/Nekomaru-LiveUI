//! WebSocket endpoint for KPM streaming.
//!
//! `GET /api/v1/ws/kpm` — upgrades to a WebSocket that pushes JSON text
//! messages `{"kpm": N}` whenever the KPM value changes.

use crate::kpm::hook::KpmState;
use crate::state::AppState;

use axum::Router;
use axum::extract::{State, WebSocketUpgrade, ws};
use axum::response::IntoResponse;
use axum::routing::get;

use std::sync::Arc;

use tokio::sync::RwLock;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/ws/kpm", get(ws_kpm))
}

async fn ws_kpm(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let kpm_arc = state.kpm_arc();
    ws.on_upgrade(move |socket| handle_kpm_ws(socket, kpm_arc))
}

async fn handle_kpm_ws(
    mut socket: ws::WebSocket,
    kpm_arc: Arc<RwLock<KpmState>>,
) {
    // Clone the watch receiver — doesn't need the lock held after clone.
    let mut rx = {
        let kpm = kpm_arc.read().await;
        kpm.notify.subscribe()
    };

    // Send the current value immediately.
    {
        let val = *rx.borrow_and_update();
        let json = match val {
            Some(kpm) => format!(r#"{{"kpm":{kpm}}}"#),
            None => r#"{"kpm":null}"#.to_owned(),
        };
        if socket.send(ws::Message::Text(json.into())).await.is_err() { return; }
    }

    // Push loop: wait for value changes.
    loop {
        if rx.changed().await.is_err() { break; } // Sender dropped.

        let val = *rx.borrow_and_update();
        let json = match val {
            Some(kpm) => format!(r#"{{"kpm":{kpm}}}"#),
            None => r#"{"kpm":null}"#.to_owned(),
        };
        if socket.send(ws::Message::Text(json.into())).await.is_err() { break; }
    }

    drop(socket);
}
