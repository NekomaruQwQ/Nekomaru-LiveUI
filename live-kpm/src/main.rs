//! `live-kpm` — standalone keystroke counter for Nekomaru LiveUI.
//!
//! Installs a `WH_KEYBOARD_LL` system-wide hook on a dedicated message pump
//! thread, polls the keystroke counter every 50ms, computes KPM via a 5-second
//! sliding window, and writes `KpmUpdate` messages to stdout via `live-protocol`
//! framing.  Pipe through `live-ws` for WebSocket delivery.
//!
//! ## Privacy-by-Design
//!
//! The hook callback never inspects key identity beyond a transient bitset
//! index for auto-repeat suppression.  No key codes are logged or transmitted.
//!
//! ## Usage
//!
//! ```text
//! live-kpm | live-ws --server ws://machineA:3000/api/v1/ws/kpm/input
//! live-kpm > kpm.bin  # dump for testing
//! ```

mod calculator;
mod hook;
mod message_pump;

use calculator::KpmCalculator;
use message_pump::MessagePump;

use live_protocol::{MessageType, write_message};

use std::io::BufWriter;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Constants ───────────────────────────────────────────────────────────────

/// Batch interval: how often we poll the atomic keystroke counter.
const BATCH_INTERVAL_MS: u64 = 50;

/// Sliding window duration for KPM calculation.
const WINDOW_DURATION_MS: u64 = 5000;

// ── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    if let Err(e) = run() {
        log::error!("fatal: {e}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    // Start the message pump with the keyboard hook.
    let _pump = MessagePump::start(hook::install_hook)?;

    log::info!("KPM capture started (batch: {BATCH_INTERVAL_MS}ms, window: {WINDOW_DURATION_MS}ms)");

    let mut calculator = KpmCalculator::new(WINDOW_DURATION_MS, BATCH_INTERVAL_MS);
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut last_kpm: i64 = 0;

    let interval = Duration::from_millis(BATCH_INTERVAL_MS);

    loop {
        std::thread::sleep(interval);

        let count = hook::take_keystroke_count();
        let timestamp_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        calculator.push_batch(timestamp_us, count);
        let kpm = calculator.get_kpm().round() as i64;

        // Only write when the value changes (avoids flooding stdout at 20Hz
        // with identical values).
        if kpm != last_kpm {
            let payload = kpm.to_le_bytes();
            if let Err(e) = write_message(
                &mut writer, MessageType::KpmUpdate, 0, &payload) {
                // Stdout broken (pipe closed) — exit cleanly.
                log::info!("stdout closed: {e}");
                break;
            }
            last_kpm = kpm;
        }
    }

    Ok(())
}
