/**
 * Video WebSocket relay.
 *
 * - Encoder-facing:  `WS /api/v1/ws/video/:id/input` — receives binary frames from live-ws.
 * - Frontend-facing: `WS /api/v1/ws/video/:id`       — pushes frames to viewers.
 *
 * The server does NOT buffer frames.  It relays them.  It caches:
 * - Last CodecParams message per stream (for the /init endpoint)
 * - Last keyframe per stream (sent to late-joining frontend clients)
 *
 * Stream presence is derived from connected encoder WS sockets.
 */

import { Hono } from "hono";
import { upgradeWebSocket } from "hono/bun";
import { MessageType, Flags, getMessageType, getFlags, HEADER_SIZE } from "./protocol";
import { parseCodecParams, buildCodecString, buildAvccDescriptor } from "./codec";

// ── Per-Stream State ────────────────────────────────────────────────────────

interface StreamState {
    /** Connected frontend WS clients for this stream. */
    frontendClients: Set<{ send: (data: ArrayBuffer) => void }>;
    /** Cached raw CodecParams message (for /init and late joiners). */
    cachedCodecParams: Uint8Array | null;
    /** Cached raw keyframe message (for late joiners). */
    cachedKeyframe: Uint8Array | null;
    /** Whether an encoder is currently connected. */
    encoderConnected: boolean;
}

/** Active streams keyed by stream ID. */
const streams = new Map<string, StreamState>();

function getOrCreateStream(id: string): StreamState {
    let state = streams.get(id);
    if (!state) {
        state = {
            frontendClients: new Set(),
            cachedCodecParams: null,
            cachedKeyframe: null,
            encoderConnected: false,
        };
        streams.set(id, state);
    }
    return state;
}

// ── Public API ──────────────────────────────────────────────────────────────

/** List active stream IDs (streams with an encoder connected). */
export function listStreams(): { id: string }[] {
    const result: { id: string }[] = [];
    for (const [id, state] of streams) {
        if (state.encoderConnected) {
            result.push({ id });
        }
    }
    return result;
}

// ── Routes ──────────────────────────────────────────────────────────────────

const app = new Hono();

// GET /api/v1/streams — list active streams.
app.get("/", (c) => c.json(listStreams()));

// GET /api/v1/streams/:id/init — codec params for VideoDecoder.configure().
app.get("/:id/init", (c) => {
    const id = c.req.param("id") as string;
    const state = streams.get(id);

    if (!state?.encoderConnected) {
        return c.json({ error: "stream not found" }, 404);
    }
    if (!state.cachedCodecParams) {
        return c.json({ error: "codec params not yet available" }, 503);
    }

    const { width, height, sps, pps } = parseCodecParams(state.cachedCodecParams);
    const codec = buildCodecString(sps);
    const description = buildAvccDescriptor(sps, pps);

    // Base64-encode the avcC descriptor for JSON transport.
    const descriptionBase64 = Buffer.from(description).toString("base64");

    return c.json({ codec, width, height, description: descriptionBase64 });
});

// WS /api/v1/ws/video/:id/input — encoder input (from live-ws).
app.get(
    "/ws/:id/input",
    upgradeWebSocket((c) => {
        const id = c.req.param("id") as string;
        return {
            onOpen(_event, ws) {
                const state = getOrCreateStream(id);
                state.encoderConnected = true;
                console.log(`[video] encoder connected: ${id}`);
            },

            onMessage(event, _ws) {
                const state = streams.get(id);
                if (!state) return;

                const raw = event.data;
                if (!(raw instanceof ArrayBuffer) && !ArrayBuffer.isView(raw)) return;
                const bytes = new Uint8Array(
                    raw instanceof ArrayBuffer ? raw : raw.buffer);

                if (bytes.length < HEADER_SIZE) return;

                const msgType = getMessageType(bytes);
                const msgFlags = getFlags(bytes);

                // Cache CodecParams and keyframes for late joiners.
                if (msgType === MessageType.CodecParams) {
                    state.cachedCodecParams = bytes.slice();
                } else if (msgType === MessageType.Frame && (msgFlags & Flags.IS_KEYFRAME)) {
                    state.cachedKeyframe = bytes.slice();
                }

                // Fan-out to all connected frontend clients.
                const buf = bytes.buffer as ArrayBuffer;
                for (const client of state.frontendClients) {
                    try { client.send(buf); }
                    catch { state.frontendClients.delete(client); }
                }
            },

            onClose() {
                const state = streams.get(id);
                if (state) {
                    state.encoderConnected = false;
                    // Keep cached data for brief reconnects.
                    console.log(`[video] encoder disconnected: ${id}`);
                }
            },
        };
    })
);

// WS /api/v1/ws/video/:id — frontend viewer.
app.get(
    "/ws/:id",
    upgradeWebSocket((c) => {
        const id = c.req.param("id") as string;
        return {
            onOpen(_event, ws) {
                const state = getOrCreateStream(id);
                const client = { send: (data: ArrayBuffer) => ws.send(data) };
                state.frontendClients.add(client);

                // Send cached codec params + keyframe for immediate playback.
                if (state.cachedCodecParams) {
                    ws.send(state.cachedCodecParams.buffer as ArrayBuffer);
                }
                if (state.cachedKeyframe) {
                    ws.send(state.cachedKeyframe.buffer as ArrayBuffer);
                }

                console.log(`[video] frontend connected: ${id} (${state.frontendClients.size} clients)`);

                // Store the client ref on the ws for cleanup in onClose.
                (ws as any).__client = client;
            },

            onClose(_event, ws) {
                const state = streams.get(id);
                const client = (ws as any).__client;
                if (state && client) {
                    state.frontendClients.delete(client);
                    console.log(`[video] frontend disconnected: ${id} (${state.frontendClients.size} clients)`);
                }
            },
        };
    })
);

export default app;
