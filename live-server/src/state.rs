//! Top-level shared application state.
//!
//! Wrapped in `Arc<AppState>` and passed to all Axum handlers via the `State`
//! extractor.  Each subsystem (video, audio, KPM, selector, strings) owns its
//! state behind a `tokio::sync::RwLock` for concurrent read-heavy access.

use crate::strings::store::StringStore;

use tokio::sync::RwLock;

/// Shared state for the entire server.
pub struct AppState {
    pub strings: RwLock<StringStore>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            strings: RwLock::new(StringStore::new()),
        }
    }
}
