//! Worker event endpoints.
//!
//! Internal HTTP endpoints called by capture workers to report metadata.
//!
//! ## Routes
//!
//! - `POST /internal/streams/:streamId/event` — capture switch metadata from
//!   `live-capture --mode auto`.  Updates computed strings.

use crate::state::AppState;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::Json;
use axum::routing::post;
use serde::Deserialize;

use std::sync::Arc;

// ── Computed String IDs ─────────────────────────────────────────────────

/// Human-readable label for the captured window.
const CSID_CAPTURE_INFO: &str = "$captureInfo";

/// Current capture mode (e.g. `"auto"`).
const CSID_CAPTURE_MODE: &str = "$captureMode";

/// Mode tag from the matched pattern (e.g. `"code"`, `"game"`).
const CSID_LIVE_MODE: &str = "$liveMode";

// ── Routes ──────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/internal/streams/{streamId}/event", post(stream_info))
}

// ── Stream Info ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StreamInfoBody {
    #[expect(dead_code, reason = "received but not used by the server")]
    hwnd: String,
    title: String,
    file_description: String,
    mode: Option<String>,
}

/// `POST /internal/streams/:streamId/event` — capture switch metadata.
///
/// Called by `live-capture --mode auto` on each window switch.  Updates
/// the computed strings that the frontend displays.
async fn stream_info(
    State(state): State<Arc<AppState>>,
    Path(_stream_id): Path<String>,
    Json(body): Json<StreamInfoBody>,
) -> Json<serde_json::Value> {
    let mut store = state.strings.write().await;

    let info = if body.file_description.is_empty() {
        &body.title
    } else {
        &body.file_description
    };

    store.set_computed(CSID_CAPTURE_INFO, info.clone());
    store.set_computed(CSID_CAPTURE_MODE, "auto".to_owned());

    if let Some(ref mode) = body.mode {
        store.set_computed(CSID_LIVE_MODE, mode.clone());
    } else {
        store.clear_computed(CSID_LIVE_MODE);
    }
    drop(store);

    log::info!("streamInfo: {info} (mode: {})", body.mode.as_deref().unwrap_or("none"));

    Json(serde_json::json!({ "ok": true }))
}
