// Low-level WebSocket helpers.
//
// `runReconnectingWS` runs the standard reconnect-with-backoff loop used by
// every viewer-side stream (video, audio, KPM, strings).  Callers supply a
// per-connection body that owns message handling and decides when the
// connection has ended (typically by resolving on `onclose`).
//
// `wsMessages` is an async-generator helper for tight binary message loops
// (video) — yields ArrayBuffer messages until the WS closes or the signal
// fires.  Use it inside a `runReconnectingWS` body.

const INITIAL_DELAY_MS = 100;
const MAX_DELAY_MS = 5000;

/// Run a reconnecting WebSocket loop with exponential backoff (100→5000 ms).
///
/// `body` is invoked once per connection attempt with the freshly-constructed
/// WebSocket.  It owns:
///   - message handling (`ws.onmessage` or addEventListener)
///   - deciding when this connection is done (typically by resolving on
///     `ws.onclose` / `onerror`, or by awaiting a `wsMessages` generator)
///   - any per-connection setup/teardown (worklets, decoders, etc.)
///
/// Backoff resets the moment a connection successfully opens.  The loop ends
/// when `signal` aborts.
export async function runReconnectingWS(
    path: string,
    signal: AbortSignal,
    body: (ws: WebSocket) => Promise<void>,
): Promise<void> {
    let delay = INITIAL_DELAY_MS;

    while (!signal.aborted) {
        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(`${proto}//${location.host}${path}`);
        // Safe default for binary streams.  Has no effect on text frames, so
        // setting it unconditionally lets text-only consumers (KPM, strings)
        // ignore the option entirely.
        ws.binaryType = "arraybuffer";

        // Reset backoff the instant the socket opens — independent of how
        // body chooses to observe the open.
        ws.addEventListener("open", () => { delay = INITIAL_DELAY_MS; }, { once: true });

        const onAbort = () => ws.close();
        signal.addEventListener("abort", onAbort, { once: true });

        try {
            await body(ws);
        } catch (e) {
            // Body threw — log and fall through to backoff.  Most bodies
            // shouldn't throw (they resolve on close/error), but a buggy
            // handler shouldn't kill the whole reconnect loop.
            console.warn("runReconnectingWS: body threw:", e);
        } finally {
            signal.removeEventListener("abort", onAbort);
            if (ws.readyState !== WebSocket.CLOSED) ws.close();
        }

        if (signal.aborted) break;
        await sleep(delay);
        delay = Math.min(delay * 2, MAX_DELAY_MS);
    }
}

/// Async generator yielding binary ArrayBuffer messages from a WebSocket.
/// Ends when the WebSocket closes, errors, or the signal fires.
///
/// Intended for use inside a `runReconnectingWS` body — the generator's
/// completion signals the body that the connection is over.
export async function* wsMessages(
    ws: WebSocket,
    signal: AbortSignal,
): AsyncGenerator<ArrayBuffer> {
    const queue: ArrayBuffer[] = [];
    let resolve: (() => void) | null = null;
    let done = false;

    ws.onmessage = (ev: MessageEvent) => {
        if (ev.data instanceof ArrayBuffer) {
            queue.push(ev.data);
            resolve?.();
        }
    };
    ws.onclose = () => { done = true; resolve?.(); };
    ws.onerror = () => { done = true; resolve?.(); };

    const onAbort = () => { ws.close(); };
    signal.addEventListener("abort", onAbort, { once: true });

    try {
        while (!done && !signal.aborted) {
            if (queue.length > 0) {
                // biome-ignore lint/style/noNonNullAssertion: length check above
                yield queue.shift()!;
            } else {
                await new Promise<void>(r => { resolve = r; });
                resolve = null;
            }
        }
    } finally {
        signal.removeEventListener("abort", onAbort);
    }
}

function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
