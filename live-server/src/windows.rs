//! Window enumeration endpoint.
//!
//! Calls the `enumerate-windows` crate directly — no child process spawn
//! needed.  This replaces the old pattern of shelling out to
//! `live-video.exe --enumerate-windows`.

use crate::state::AppState;

use axum::Router;
use axum::routing::get;
use axum::response::Json;

use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/streams/windows", get(list_windows))
}

/// `GET /api/v1/streams/windows` — list capturable windows.
///
/// Runs synchronously on a blocking thread to avoid stalling the tokio
/// runtime (Win32 enumeration calls are blocking).
async fn list_windows() -> Json<Vec<enumerate_windows::WindowInfo>> {
    let windows = tokio::task::spawn_blocking(enumerate_windows::enumerate_windows)
        .await
        .unwrap_or_default();
    Json(windows)
}
