//! WebSocket endpoint for string store streaming.
//!
//! `GET /api/v1/ws/strings` — upgrades to a WebSocket that pushes a full
//! JSON snapshot of all strings (user + computed) whenever any value changes.

use crate::state::AppState;
use crate::strings::store::StringStore;

use axum::Router;
use axum::extract::{State, WebSocketUpgrade, ws};
use axum::response::IntoResponse;
use axum::routing::get;

use std::sync::Arc;

use tokio::sync::RwLock;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/ws/strings", get(ws_strings))
}

async fn ws_strings(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let strings_arc = state.strings_arc();
    ws.on_upgrade(move |socket| handle_strings_ws(socket, strings_arc))
}

async fn handle_strings_ws(
    mut socket: ws::WebSocket,
    strings_arc: Arc<RwLock<StringStore>>,
) {
    // Clone the watch receiver.
    let mut rx = {
        let store = strings_arc.read().await;
        store.notify.subscribe()
    };

    // Send the current snapshot immediately.
    {
        let store = strings_arc.read().await;
        let all = store.get_all();
        drop(store);
        let json = serde_json::to_string(&all).expect("JSON serialization");
        if socket.send(ws::Message::Text(json.into())).await.is_err() { return; }
    }

    // Push loop: wait for version bumps, then send full snapshots.
    loop {
        if rx.changed().await.is_err() { break; }
        // Mark seen so we don't re-trigger immediately.
        let _ = *rx.borrow_and_update();

        let store = strings_arc.read().await;
        let all = store.get_all();
        drop(store);

        let json = serde_json::to_string(&all).expect("JSON serialization");
        if socket.send(ws::Message::Text(json.into())).await.is_err() { break; }
    }

    drop(socket);
}
