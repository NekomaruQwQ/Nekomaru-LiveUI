//! HTTP routes for video stream management.
//!
//! - `GET    /api/v1/streams`            — list active streams
//! - `POST   /api/v1/streams`            — create a new capture
//! - `DELETE /api/v1/streams/:id`        — destroy a capture
//! - `GET    /api/v1/streams/:id/init`   — codec string + avcC descriptor
//! - `GET    /api/v1/streams/:id/frames` — AVCC frames (binary, polling)

use crate::state::AppState;
use crate::video::buffer::{build_avcc_descriptor, build_codec_string};
use crate::video::process::CropParams;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{delete, get};

use base64::Engine as _;
use serde::Deserialize;

use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/streams", get(list_streams).post(create_stream))
        .route("/api/v1/streams/{id}", delete(destroy_stream))
        .route("/api/v1/streams/{id}/init", get(get_init))
        .route("/api/v1/streams/{id}/frames", get(get_frames))
}

// ── List ─────────────────────────────────────────────────────────────────────

async fn list_streams(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let registry = state.streams().await;
    Json(registry.list())
}

// ── Create ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(untagged)]
enum CreateBody {
    Resample { hwnd: String, width: u32, height: u32 },
    Crop {
        hwnd: String,
        #[serde(rename = "cropMinX")]
        crop_min_x: u32,
        #[serde(rename = "cropMinY")]
        crop_min_y: u32,
        #[serde(rename = "cropMaxX")]
        crop_max_x: u32,
        #[serde(rename = "cropMaxY")]
        crop_max_y: u32,
    },
}

async fn create_stream(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBody>,
) -> impl IntoResponse {
    let mut registry = state.streams_mut().await;
    let arc = state.streams_arc();

    let id = match body {
        CreateBody::Resample { hwnd, width, height } =>
            registry.create_stream(&hwnd, width, height, &arc),
        CreateBody::Crop { hwnd, crop_min_x, crop_min_y, crop_max_x, crop_max_y } => {
            let crop = CropParams { min_x: crop_min_x, min_y: crop_min_y, max_x: crop_max_x, max_y: crop_max_y };
            registry.create_crop_stream(&hwnd, &crop, None, &arc)
        }
    };
    drop(registry);

    (StatusCode::CREATED, Json(serde_json::json!({ "id": id })))
}

// ── Destroy ──────────────────────────────────────────────────────────────────

async fn destroy_stream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut registry = state.streams_mut().await;
    if !registry.streams.contains_key(&id) {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "stream not found" }))).into_response();
    }
    registry.destroy_stream(&id);
    drop(registry);
    Json(serde_json::json!({ "ok": true })).into_response()
}

// ── Init (codec string + avcC descriptor) ───────────────────────────────────

/// `GET /api/v1/streams/:id/init` — pre-built decoder configuration.
///
/// Returns the `avc1.PPCCLL` codec string and a base64-encoded
/// AVCDecoderConfigurationRecord (avcC) so the frontend can call
/// `VideoDecoder.configure()` with zero H.264 format knowledge.
async fn get_init(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let registry = state.streams().await;
    let Some(stream) = registry.streams.get(&id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "stream not found" }))).into_response();
    };

    let Some(params) = stream.buffer.get_codec_params() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({ "error": "codec params not yet available" }))).into_response();
    };

    let codec = build_codec_string(&params.sps);
    let avcc = build_avcc_descriptor(&params.sps, &params.pps);
    let width = params.width;
    let height = params.height;
    drop(registry);

    let b64 = base64::engine::general_purpose::STANDARD;
    Json(serde_json::json!({
        "codec": codec,
        "width": width,
        "height": height,
        "description": b64.encode(&avcc),
    })).into_response()
}

// ── Frames (binary) ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FramesQuery {
    after: Option<String>,
}

/// `GET /api/v1/streams/:id/frames?after=N` — AVCC frame data.
///
/// Response layout (all little-endian unless noted):
/// ```text
/// [u32 LE: generation][u32 LE: num_frames]
/// per frame:
///   [u32 LE: sequence]
///   [u64 LE: timestamp_us]
///   [u8:     is_keyframe]
///   [u32 LE: avcc_payload_length]
///   [avcc_payload bytes]          ← ready for EncodedVideoChunk.data
/// ```
async fn get_frames(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<FramesQuery>,
) -> impl IntoResponse {
    let registry = state.streams().await;
    let Some(stream) = registry.streams.get(&id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "stream not found" }))).into_response();
    };

    let after: u32 = query.after
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let frames = stream.buffer.get_frames_after(after);
    let generation = stream.generation;

    // Pre-compute total size: 8-byte header + (17 + payload) per frame.
    // Per-frame envelope: sequence(4) + timestamp(8) + keyframe(1) + len(4) = 17.
    let mut total_size: usize = 8;
    for f in &frames {
        total_size += 17 + f.payload.len();
    }

    let mut buf = vec![0u8; total_size];
    let mut pos = 0;

    // Header: generation + frame count.
    buf[pos..pos + 4].copy_from_slice(&generation.to_le_bytes());
    pos += 4;
    buf[pos..pos + 4].copy_from_slice(&(frames.len() as u32).to_le_bytes());
    pos += 4;

    // Each frame: sequence, timestamp, keyframe flag, AVCC payload.
    for f in &frames {
        buf[pos..pos + 4].copy_from_slice(&f.sequence.to_le_bytes());
        pos += 4;
        buf[pos..pos + 8].copy_from_slice(&f.timestamp_us.to_le_bytes());
        pos += 8;
        buf[pos] = u8::from(f.is_keyframe);
        pos += 1;
        buf[pos..pos + 4].copy_from_slice(&(f.payload.len() as u32).to_le_bytes());
        pos += 4;
        buf[pos..pos + f.payload.len()].copy_from_slice(&f.payload);
        pos += f.payload.len();
    }

    drop(registry);

    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(buf))
        .unwrap()
        .into_response()
}
