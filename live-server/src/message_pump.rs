//! Reusable Win32 message pump on a dedicated OS thread.
//!
//! Some Win32 APIs — notably low-level hooks (`WH_KEYBOARD_LL`) — require
//! the installing thread to run a message pump.  This module provides a
//! [`MessagePump`] that spawns a dedicated thread, runs the standard
//! `GetMessageW` / `TranslateMessage` / `DispatchMessageW` loop, and offers
//! lifecycle hooks for thread-bound setup and teardown.
//!
//! The pump thread is a plain `std::thread` — completely independent of the
//! tokio async runtime.  The only cross-thread communication should go through
//! lock-free primitives (e.g. `AtomicU32`).

#![expect(clippy::multiple_unsafe_ops_per_block, reason = "Windows API calls")]

use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use anyhow::Context as _;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;

/// A Win32 message pump running on a dedicated OS thread.
///
/// The pump is started via [`MessagePump::start`], which accepts an `init`
/// closure that runs on the pump thread before the message loop.  The closure
/// returns a cleanup callback invoked after the loop exits (e.g. to unhook).
///
/// Dropping the pump posts `WM_QUIT` to the thread and joins it.
pub struct MessagePump {
    thread: Option<JoinHandle<()>>,
    /// Win32 thread ID of the pump thread — used by [`Self::stop`] to post
    /// `WM_QUIT` cross-thread via `PostThreadMessageW`.
    pump_thread_id: u32,
}

impl MessagePump {
    /// Spawn a dedicated OS thread and run a Win32 message pump on it.
    ///
    /// `init` executes on the pump thread *before* the message loop starts.
    /// Use it to install hooks or perform other thread-bound setup.  It must
    /// return a boxed cleanup closure that runs *after* the message loop exits
    /// (e.g. to call `UnhookWindowsHookEx`).  If `init` returns `Err`, the
    /// pump thread exits immediately and the error propagates to the caller.
    ///
    /// This call blocks until `init` completes on the pump thread.
    pub fn start(
        init: impl FnOnce() -> anyhow::Result<Box<dyn FnOnce()>> + Send + 'static,
    ) -> anyhow::Result<Self> {
        // Rendezvous channel: the pump thread sends back its Win32 thread ID
        // on success, or an error if `init` failed.
        let (tx, rx) = mpsc::sync_channel::<anyhow::Result<u32>>(0);

        let thread = thread::Builder::new()
            .name("message-pump".into())
            .spawn(move || {
                let cleanup = match init() {
                    Ok(cleanup) => {
                        // SAFETY: returns the calling thread's Win32 thread ID.
                        let thread_id = unsafe { GetCurrentThreadId() };
                        // Signal the spawning thread that we're ready.
                        if tx.send(Ok(thread_id)).is_err() { return; }
                        cleanup
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };

                pump_messages();
                cleanup();
            })
            .context("failed to spawn message pump thread")?;

        let pump_thread_id = rx
            .recv()
            .context("message pump thread died before signaling readiness")??;

        Ok(Self { thread: Some(thread), pump_thread_id })
    }

    /// Post `WM_QUIT` to the pump thread and join it.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn stop(&mut self) {
        if let Some(thread) = self.thread.take() {
            // SAFETY: `pump_thread_id` is a valid Win32 thread ID obtained
            // from `GetCurrentThreadId()` on the pump thread.  Posting
            // `WM_QUIT` causes `GetMessageW` to return FALSE, exiting the
            // message loop.
            let _ = unsafe {
                PostThreadMessageW(self.pump_thread_id, WM_QUIT, WPARAM(0), LPARAM(0))
            };
            let _ = thread.join();
        }
    }
}

impl Drop for MessagePump {
    fn drop(&mut self) { self.stop(); }
}

/// Run the standard Win32 message pump until `WM_QUIT`.
fn pump_messages() {
    // SAFETY: `msg` is zero-initialized and only used as an out-parameter for
    // `GetMessageW`.  The loop exits cleanly when `GetMessageW` returns FALSE
    // (i.e. on `WM_QUIT`).
    unsafe {
        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&raw mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
}
