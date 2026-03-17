//! KPM capture process manager.
//!
//! Spawns `live-kpm.exe`, reads 12-byte binary batches from stdout via
//! `live_kpm::read_batch()`, and pushes them into the `KpmCalculator`.

use crate::kpm::calculator::KpmCalculator;

use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use job_object::JobObject;
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;

use crate::constant::{KPM_BATCH_INTERVAL_MS, KPM_WINDOW_DURATION_MS};

// ── KpmState ─────────────────────────────────────────────────────────────────

pub struct KpmState {
    pub calculator: KpmCalculator,
    pub active: bool,
    pub child: Option<Child>,
    pub reader_handle: Option<JoinHandle<()>>,
    /// Watch channel carrying the latest KPM value.  `None` when the capture
    /// process is not running.  WebSocket handlers clone the receiver and
    /// `changed().await` to push updates.
    pub notify: watch::Sender<Option<i64>>,
}

impl KpmState {
    pub fn new() -> Self {
        let (notify, _) = watch::channel(None);
        Self {
            calculator: KpmCalculator::new(KPM_WINDOW_DURATION_MS, KPM_BATCH_INTERVAL_MS),
            active: false,
            child: None,
            reader_handle: None,
            notify,
        }
    }

    /// Start the KPM capture process.
    pub fn start(&mut self, exe_path: &str, job: &JobObject, state_arc: &Arc<RwLock<Self>>) {
        if self.active { return; }

        let mut child = Command::new(exe_path)
            .arg("--batch-interval")
            .arg(KPM_BATCH_INTERVAL_MS.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {exe_path}: {e}"));

        if let Err(e) = job.assign(&child) {
            log::warn!("failed to assign to job object: {e}");
        }

        let stdout = child.stdout.take().expect("stdout must be piped");

        let state_clone = Arc::clone(state_arc);

        // Stdout reader: reads fixed 12-byte binary batches.
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match live_kpm::read_batch(&mut reader) {
                    Ok(Some(batch)) => {
                        let mut state = state_clone.blocking_write();
                        state.calculator.push_batch(batch.t, batch.c);
                        let kpm = state.calculator.get_kpm().round() as i64;
                        state.notify.send_replace(Some(kpm));
                        drop(state);
                    }
                    Ok(None) => {
                        log::info!("stdout EOF");
                        break;
                    }
                    Err(e) => {
                        log::error!("read error: {e}");
                        break;
                    }
                }
            }

            let mut state = state_clone.blocking_write();
            state.active = false;
            state.calculator.reset();
            state.notify.send_replace(None);
        });

        self.child = Some(child);
        self.reader_handle = Some(reader_handle);
        self.active = true;

        log::info!("started (batch: {KPM_BATCH_INTERVAL_MS}ms, window: {KPM_WINDOW_DURATION_MS}ms)");
    }

    pub fn stop(&mut self) {
        if !self.active { return; }

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }

        self.active = false;
        self.calculator.reset();
        self.notify.send_replace(None);
        log::info!("stopped");
    }
}

impl Drop for KpmState {
    fn drop(&mut self) { self.stop(); }
}
