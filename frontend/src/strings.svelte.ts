// String-store singleton.
//
// Subscribes to /api/strings/ws and exposes the latest snapshot as a
// reactive record.  Consumers read `strings.value.someKey` and Svelte
// tracks per-key lookup automatically through `$state`.

import { stringsWsLoop } from "./strings-loop";

class StringsStore {
    /// All server-managed strings as a key-value record.
    /// Keys may be user-provided (`marquee`, `message`) or computed (`$liveMode`,
    /// `$captureMode`, `$captureInfo`, `$timestamp`).
    value = $state<Record<string, string>>({});
}

export const strings = new StringsStore();

// Module-level singleton — lifetime = page lifetime, no AbortSignal needed.
// The controller is constructed but never aborted, so the loop runs forever.
void stringsWsLoop(new AbortController().signal, snapshot => {
    strings.value = snapshot;
});
