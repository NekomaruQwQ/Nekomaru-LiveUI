// String-store singleton.
//
// Polls GET /api/strings every 2 seconds and exposes the result as a
// reactive record.  Consumers read `strings.value.someKey` and Svelte
// tracks per-keys lookup automatically through `$state`.

import { fetchStrings } from "./strings-api";

const POLL_INTERVAL_MS = 2000;

class StringsStore {
    /// All server-managed strings as a key-value record.
    /// Keys may be user-provided (`marquee`, `message`) or computed (`$liveMode`,
    /// `$captureMode`, `$captureInfo`, `$timestamp`).
    value = $state<Record<string, string>>({});
}

export const strings = new StringsStore();

async function poll() {
    try {
        strings.value = await fetchStrings();
    } catch (e) {
        console.error("Failed to poll strings:", e);
    }
}

void poll();
setInterval(poll, POLL_INTERVAL_MS);
