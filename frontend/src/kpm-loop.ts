// KPM WebSocket connection loop.
//
// Extracted from the React component so the Svelte version of KpmMeter only
// holds presentation logic.  This module is framework-agnostic.

const INITIAL_DELAY_MS = 100;
const MAX_DELAY_MS = 5000;

/// Connect to the KPM WebSocket with auto-reconnect and exponential backoff.
/// Calls `onValue` for each received KPM update (null = process not running).
export async function kpmWsLoop(
    signal: AbortSignal,
    onValue: (kpm: number | null) => void,
): Promise<void> {
    let delay = INITIAL_DELAY_MS;

    while (!signal.aborted) {
        try {
            const connected = await runOneKpmConnection(signal, onValue);
            // Reset backoff on successful connection (the WS opened and
            // eventually closed normally — not a connection failure).
            if (connected) delay = INITIAL_DELAY_MS;
        } catch {
            // Connection failed — fall through to backoff.
        }

        if (signal.aborted) break;
        await sleep(delay);
        delay = Math.min(delay * 2, MAX_DELAY_MS);
    }
}

/// Open a single WS connection to /api/kpm and process messages.
/// Returns true if the connection was successfully established (for backoff
/// reset), false if it failed before opening.
function runOneKpmConnection(
    signal: AbortSignal,
    onValue: (kpm: number | null) => void,
): Promise<boolean> {
    return new Promise<boolean>((resolve, reject) => {
        if (signal.aborted) { resolve(false); return; }

        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(`${proto}//${location.host}/api/kpm`);

        const onAbort = () => ws.close();
        signal.addEventListener("abort", onAbort, { once: true });

        ws.onmessage = (ev: MessageEvent) => {
            if (typeof ev.data === "string") {
                const data = JSON.parse(ev.data) as { kpm: number | null };
                onValue(data.kpm);
            }
        };

        ws.onclose = () => {
            signal.removeEventListener("abort", onAbort);
            resolve(true);
        };

        ws.onerror = () => {
            signal.removeEventListener("abort", onAbort);
            reject(new Error("WebSocket error"));
        };
    });
}

function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
