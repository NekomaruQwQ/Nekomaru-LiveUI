//! Low-level keyboard hook for KPM capture.
//!
//! Installs a `WH_KEYBOARD_LL` system-wide hook on the message pump thread.
//! The hook callback increments a static atomic counter on every initial
//! key-down event (auto-repeat suppressed via a thread-local bitset).
//!
//! ## Privacy-by-Design
//!
//! The hook callback reads `vkCode` **only** to maintain a pressed/released
//! bitset for auto-repeat suppression.  The key code is used as a transient
//! index and is never logged, stored, or transmitted.

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ── Keyboard Hook ───────────────────────────────────────────────────────────

/// Keystroke counter shared between the hook callback (pump thread) and the
/// timer task (main thread).  The hook increments; the timer reads + resets.
static COUNTER: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// Bitset tracking which virtual key codes (0-255) are currently held down.
    /// Used by the hook callback to suppress auto-repeat events.
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
/// Must be called on a thread with a message pump.  Returns a cleanup
/// closure that calls `UnhookWindowsHookEx`.
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
pub fn take_keystroke_count() -> u32 {
    COUNTER.swap(0, Ordering::Relaxed)
}

/// Low-level keyboard hook callback.
///
/// # Safety
///
/// Called by the OS with valid `ncode`, `wparam`, `lparam` per the
/// `WH_KEYBOARD_LL` contract.
unsafe extern "system" fn keyboard_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 {
        // SAFETY: `lparam` points to a valid `KBDLLHOOKSTRUCT`.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = info.vkCode as usize;

        if vk < 256 {
            let is_keydown = wparam.0 == WM_KEYDOWN as usize
                || wparam.0 == WM_SYSKEYDOWN as usize;
            let is_keyup = wparam.0 == WM_KEYUP as usize
                || wparam.0 == WM_SYSKEYUP as usize;

            if is_keydown && mark_pressed(vk) {
                COUNTER.fetch_add(1, Ordering::Relaxed);
            } else if is_keyup {
                mark_released(vk);
            }
        }
    }

    // SAFETY: Always pass the event to the next hook in the chain.
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}
