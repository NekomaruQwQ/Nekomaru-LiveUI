//! In-process KPM capture via a low-level keyboard hook.
//!
//! Replaces the former `live-kpm.exe` child process.  The keyboard hook runs
//! on the [`MessagePump`] thread; a tokio timer task polls the atomic counter
//! every batch interval and feeds the [`KpmCalculator`].
//!
//! ## Privacy-by-Design
//!
//! The hook callback reads `vkCode` **only** to maintain a pressed/released
//! bitset for auto-repeat suppression.  The key code is used as a transient
//! index and is never logged, stored, or transmitted — making it structurally
//! impossible for this process to act as a keylogger.

use crate::constant::{KPM_BATCH_INTERVAL_MS, KPM_WINDOW_DURATION_MS};
use crate::kpm::calculator::KpmCalculator;
use crate::message_pump::MessagePump;

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ── Keyboard Hook ───────────────────────────────────────────────────────────

/// Keystroke counter shared between the hook callback (pump thread) and the
/// timer task (tokio runtime).  The hook increments; the timer reads + resets.
static COUNTER: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// Bitset tracking which virtual key codes (0–255) are currently held down.
    /// Used by the hook callback to suppress auto-repeat events — only the
    /// initial key-down is counted.  Accessed exclusively on the pump thread.
    static PRESSED_KEYS: Cell<[u64; 4]> = const { Cell::new([0u64; 4]) };
}

/// Mark a virtual key code as pressed.  Returns `true` if this is a new press
/// (was not already held), `false` if it was already down (auto-repeat).
fn mark_pressed(vk: usize) -> bool {
    PRESSED_KEYS.with(|keys| {
        let mut bits = keys.get();
        let (word, bit) = (vk / 64, vk % 64);
        let was_pressed = bits[word] & (1u64 << bit) != 0;
        bits[word] |= 1u64 << bit;
        keys.set(bits);
        !was_pressed
    })
}

/// Mark a virtual key code as released.
fn mark_released(vk: usize) {
    PRESSED_KEYS.with(|keys| {
        let mut bits = keys.get();
        let (word, bit) = (vk / 64, vk % 64);
        bits[word] &= !(1u64 << bit);
        keys.set(bits);
    })
}

/// Install the low-level keyboard hook on the current thread.
///
/// Must be called on a thread with a message pump — the OS dispatches hook
/// callbacks to the installing thread, but only if it's pumping messages.
///
/// Returns a cleanup closure that calls `UnhookWindowsHookEx`.  The closure's
/// signature matches [`MessagePump::start`]'s `init` parameter.
pub fn install_hook() -> anyhow::Result<Box<dyn FnOnce()>> {
    // SAFETY: `keyboard_hook_proc` follows the `HOOKPROC` calling convention
    // and always calls `CallNextHookEx`.
    let hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0)
    }
    .map_err(|e| anyhow::anyhow!("failed to install keyboard hook: {e}"))?;

    log::info!("keyboard hook installed");

    Ok(Box::new(move || {
        // SAFETY: `hook` is a valid handle from `SetWindowsHookExW`.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
        log::info!("keyboard hook removed");
    }))
}

/// Atomically read and reset the keystroke counter.
///
/// Called by the timer task every batch interval to drain accumulated counts.
pub fn take_keystroke_count() -> u32 {
    COUNTER.swap(0, Ordering::Relaxed)
}

/// Low-level keyboard hook callback.
///
/// # Privacy-by-design
///
/// This function reads `vkCode` solely to index a pressed/released bitset for
/// auto-repeat suppression.  The key code is used as a transient array index
/// and is never logged, stored, or transmitted.
///
/// # Safety
///
/// Called by the OS with valid `ncode`, `wparam`, `lparam` per the
/// `WH_KEYBOARD_LL` contract.  Must return the result of `CallNextHookEx`
/// promptly (Windows removes hooks that don't return within ~200ms).
unsafe extern "system" fn keyboard_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 {
        // SAFETY: `lparam` points to a valid `KBDLLHOOKSTRUCT` per the
        // `WH_KEYBOARD_LL` contract.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = info.vkCode as usize;

        if vk < 256 {
            let is_keydown = wparam.0 == WM_KEYDOWN as usize
                || wparam.0 == WM_SYSKEYDOWN as usize;
            let is_keyup = wparam.0 == WM_KEYUP as usize
                || wparam.0 == WM_SYSKEYUP as usize;

            if is_keydown && mark_pressed(vk) {
                // Initial press only — auto-repeats are suppressed.
                COUNTER.fetch_add(1, Ordering::Relaxed);
            } else if is_keyup {
                mark_released(vk);
            }
        }
    }

    // SAFETY: Always pass the event to the next hook in the chain.
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

// ── KpmState ────────────────────────────────────────────────────────────────

/// In-process KPM capture state.
///
/// Owns the message pump (which hosts the keyboard hook) and a tokio timer
/// task that polls the atomic counter and feeds the sliding window calculator.
#[expect(clippy::partial_pub_fields, reason = "pump/timer are internal lifecycle state")]
pub struct KpmState {
    pub calculator: KpmCalculator,
    pub active: bool,
    pump: Option<MessagePump>,
    timer_handle: Option<JoinHandle<()>>,
    /// Watch channel carrying the latest KPM value.  `None` when capture is
    /// not running.  WebSocket handlers clone the receiver and
    /// `changed().await` to push updates.
    pub notify: watch::Sender<Option<i64>>,
}

impl KpmState {
    pub fn new() -> Self {
        let (notify, _) = watch::channel(None);
        Self {
            calculator: KpmCalculator::new(KPM_WINDOW_DURATION_MS, KPM_BATCH_INTERVAL_MS),
            active: false,
            pump: None,
            timer_handle: None,
            notify,
        }
    }

    /// Start KPM capture: spawn the message pump with a keyboard hook, then
    /// start a tokio timer that polls the counter every batch interval.
    pub fn start(&mut self, state_arc: &Arc<RwLock<Self>>) {
        if self.active { return; }

        let pump = MessagePump::start(install_hook)
            .unwrap_or_else(|e| panic!("failed to start KPM message pump: {e}"));

        let state_clone = Arc::clone(state_arc);
        let interval = Duration::from_millis(KPM_BATCH_INTERVAL_MS);

        let timer_handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let count = take_keystroke_count();
                let timestamp_us = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros() as u64;

                let mut state = state_clone.write().await;
                state.calculator.push_batch(timestamp_us, count);
                let kpm = state.calculator.get_kpm().round() as i64;
                state.notify.send_replace(Some(kpm));
            }
        });

        self.pump = Some(pump);
        self.timer_handle = Some(timer_handle);
        self.active = true;

        log::info!("started (batch: {KPM_BATCH_INTERVAL_MS}ms, window: {KPM_WINDOW_DURATION_MS}ms)");
    }

    /// Stop KPM capture: abort the timer task, then stop the message pump
    /// (which unhooks the keyboard hook via the cleanup closure).
    pub fn stop(&mut self) {
        if !self.active { return; }

        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
        }
        // MessagePump::drop posts WM_QUIT and joins the thread.
        self.pump.take();

        self.active = false;
        self.calculator.reset();
        self.notify.send_replace(None);
        log::info!("stopped");
    }
}

impl Drop for KpmState {
    fn drop(&mut self) { self.stop(); }
}
