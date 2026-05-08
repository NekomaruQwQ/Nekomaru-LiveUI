// Strings WebSocket connection loop.
//
// Connects to /api/strings/ws and forwards each pushed snapshot to a
// callback.  Mirrors kpm-loop.ts so the auto-reconnect / backoff
// behavior stays consistent across the app.

const INITIAL_DELAY_MS = 100;
const MAX_DELAY_MS = 5000;

/// Connect to the strings WebSocket with auto-reconnect and exponential backoff.
/// Calls `onSnapshot` for each received snapshot (the full key→value map, same
/// shape as `GET /api/strings`).
export async function stringsWsLoop(
    signal: AbortSignal,
    onSnapshot: (snapshot: Record<string, string>) => void,
): Promise<void> {
    let delay = INITIAL_DELAY_MS;

    while (!signal.aborted) {
        try {
            const connected = await runOneConnection(signal, onSnapshot);
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

/// Open a single WS connection to /api/strings/ws and process messages.
/// Returns true if the connection was successfully established (for backoff
/// reset), false if it failed before opening.
function runOneConnection(
    signal: AbortSignal,
    onSnapshot: (snapshot: Record<string, string>) => void,
): Promise<boolean> {
    return new Promise<boolean>((resolve, reject) => {
        if (signal.aborted) { resolve(false); return; }

        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(`${proto}//${location.host}/api/strings/ws`);

        const onAbort = () => ws.close();
        signal.addEventListener("abort", onAbort, { once: true });

        ws.onmessage = (ev: MessageEvent) => {
            if (typeof ev.data === "string") {
                try {
                    const data = JSON.parse(ev.data) as Record<string, string>;
                    onSnapshot(data);
                } catch (e) {
                    console.error("Failed to parse strings snapshot:", e);
                }
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
