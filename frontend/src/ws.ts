// Shared WebSocket connection helper with auto-reconnect.
//
// Manages the WebSocket lifecycle: connect, receive messages, and reconnect
// with exponential backoff on disconnect/error.  Tied to an AbortSignal for
// clean teardown (React effect cleanup, component unmount, etc.).

/// Options for `connectWs`.
export interface WsOptions {
    /// WebSocket URL path (e.g. "/api/v1/ws/video/main").
    /// Automatically resolved to a full `ws://` or `wss://` URL.
    path: string;

    /// Called for each binary message (frames, audio chunks).
    onBinaryMessage?: (data: ArrayBuffer) => void;

    /// Called for each text message (JSON — KPM, strings).
    onTextMessage?: (data: string) => void;

    /// Called when the connection opens.  Use this to send an initial cursor
    /// message (e.g. `{"after": N}`).
    onOpen?: (ws: WebSocket) => void;

    /// AbortSignal for teardown — when aborted, the WS closes and no
    /// reconnection is attempted.
    signal: AbortSignal;
}

/// Connect a WebSocket with auto-reconnect and exponential backoff.
///
/// Returns immediately — the connection and reconnect loop run in the
/// background.  Call `signal.abort()` (via AbortController) to stop.
export function connectWs(opts: WsOptions): void {
    // Run the async reconnect loop without awaiting — fire and forget.
    void reconnectLoop(opts);
}

/// Build a full WebSocket URL from a path, inheriting the page's host and
/// protocol (`ws:` for `http:`, `wss:` for `https:`).
function buildWsUrl(path: string): string {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    return `${proto}//${location.host}${path}`;
}

async function reconnectLoop(opts: WsOptions): Promise<void> {
    const INITIAL_DELAY_MS = 100;
    const MAX_DELAY_MS = 5000;
    let delay = INITIAL_DELAY_MS;

    while (!opts.signal.aborted) {
        try {
            await runOneConnection(opts);
        } catch {
            // Connection failed or errored — fall through to backoff.
        }

        if (opts.signal.aborted) break;

        // Exponential backoff before reconnecting.
        await sleep(delay);
        delay = Math.min(delay * 2, MAX_DELAY_MS);
    }
}

/// Open a single WebSocket connection and process messages until it closes.
function runOneConnection(opts: WsOptions): Promise<void> {
    return new Promise<void>((resolve, reject) => {
        if (opts.signal.aborted) { resolve(); return; }

        const url = buildWsUrl(opts.path);
        const ws = new WebSocket(url);
        ws.binaryType = "arraybuffer";

        // Close on abort signal.
        const onAbort = () => ws.close();
        opts.signal.addEventListener("abort", onAbort, { once: true });

        ws.onopen = () => {
            opts.onOpen?.(ws);
        };

        ws.onmessage = (ev: MessageEvent) => {
            if (ev.data instanceof ArrayBuffer) {
                opts.onBinaryMessage?.(ev.data);
            } else if (typeof ev.data === "string") {
                opts.onTextMessage?.(ev.data);
            }
        };

        ws.onclose = () => {
            opts.signal.removeEventListener("abort", onAbort);
            resolve();
        };

        ws.onerror = () => {
            opts.signal.removeEventListener("abort", onAbort);
            reject(new Error("WebSocket error"));
        };
    });
}

// ── Low-level helpers for async WS usage ─────────────────────────────────

/// Open a WebSocket and wait for the connection to be established.
/// Rejects on error or abort.
export function openWebSocket(path: string, signal: AbortSignal): Promise<WebSocket> {
    return new Promise<WebSocket>((resolve, reject) => {
        if (signal.aborted) { reject(new Error("aborted")); return; }

        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(`${proto}//${location.host}${path}`);
        ws.binaryType = "arraybuffer";

        const onAbort = () => { ws.close(); reject(new Error("aborted")); };
        signal.addEventListener("abort", onAbort, { once: true });

        ws.onopen = () => {
            signal.removeEventListener("abort", onAbort);
            resolve(ws);
        };
        ws.onerror = () => {
            signal.removeEventListener("abort", onAbort);
            reject(new Error("WebSocket connection error"));
        };
    });
}

/// Async generator that yields binary ArrayBuffer messages from a WebSocket.
/// Ends when the WebSocket closes or the signal fires.
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
