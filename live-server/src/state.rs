//! Top-level shared application state.
//!
//! Wrapped in `Arc<AppState>` and passed to all Axum handlers via the `State`
//! extractor.  Each subsystem (video, audio, KPM, selector, strings) owns its
//! state behind a `tokio::sync::RwLock` for concurrent read-heavy access.

use crate::audio::process::AudioState;
use crate::strings::store::StringStore;
use crate::video::process::StreamRegistry;

use std::sync::Arc;

use tokio::sync::RwLock;

/// Shared state for the entire server.
pub struct AppState {
    pub strings: RwLock<StringStore>,
    /// Shared stream registry — `Arc` so child-process reader tasks can push
    /// frames back without holding a reference to the full `AppState`.
    streams_inner: Arc<RwLock<StreamRegistry>>,
    /// Shared audio state — `Arc` so the reader task can push chunks.
    audio_inner: Arc<RwLock<AudioState>>,
}

impl AppState {
    pub fn new(video_exe_path: String) -> Self {
        Self {
            strings: RwLock::new(StringStore::new()),
            streams_inner: Arc::new(RwLock::new(StreamRegistry::new(video_exe_path))),
            audio_inner: Arc::new(RwLock::new(AudioState::new())),
        }
    }

    /// Acquire a read lock on the stream registry.
    pub async fn streams(&self) -> tokio::sync::RwLockReadGuard<'_, StreamRegistry> {
        self.streams_inner.read().await
    }

    /// Acquire a write lock on the stream registry.
    pub async fn streams_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, StreamRegistry> {
        self.streams_inner.write().await
    }

    /// Get a cloneable `Arc` handle to the streams lock for child-process tasks.
    pub fn streams_arc(&self) -> Arc<RwLock<StreamRegistry>> {
        self.streams_inner.clone()
    }

    /// Acquire a read lock on the audio state.
    pub async fn audio(&self) -> tokio::sync::RwLockReadGuard<'_, AudioState> {
        self.audio_inner.read().await
    }

    /// Acquire a write lock on the audio state.
    pub async fn audio_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, AudioState> {
        self.audio_inner.write().await
    }

    /// Get a cloneable `Arc` handle to the audio lock for reader tasks.
    pub fn audio_arc(&self) -> Arc<RwLock<AudioState>> {
        self.audio_inner.clone()
    }
}
