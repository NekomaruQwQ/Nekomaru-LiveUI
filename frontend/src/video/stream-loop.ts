// Video stream loop.
//
// Receives `live-protocol` framed messages via WebSocket, decodes video frames,
// and handles codec parameter updates.  Extracted from the React component so
// the Svelte version of StreamRenderer only holds presentation logic.

import { openWebSocket, wsMessages } from "../ws";
import { H264Decoder } from "./decoder";

// ── live-protocol constants ──────────────────────────────────────────────────

/** Frame header size (bytes).  Matches the `live-protocol` Rust crate. */
const HEADER_SIZE       = 8;
const MSG_CODEC_PARAMS  = 0x01;
const MSG_FRAME         = 0x02;
const FLAG_IS_KEYFRAME  = 1 << 0;

/**
 * Stream loop: receives `live-protocol` framed messages via WebSocket,
 * decodes video frames, and handles codec parameter updates.
 *
 * Runs until the AbortSignal fires (component unmount / streamId change).
 * Reconnects with exponential backoff on disconnect.
 *
 * Each WS message is a complete `live-protocol` frame:
 * ```
 * [u8: message_type][u8: flags][u16: reserved][u32 LE: payload_length][payload]
 * ```
 *
 * Frame (0x02) payload: `[u64 LE: timestamp_us][avcc bytes]`
 * CodecParams (0x01): triggers decoder reinitialization (rare — encoder is persistent).
 */
export async function startStreamLoop(
    streamId: string,
    onFrame: (frame: VideoFrame) => void,
    signal: AbortSignal,
): Promise<void> {
    console.log("StreamLoop: Starting stream loop");

    // Create the initial decoder.  fetchInit inside init() retries on 503
    // and 404, so this blocks until the stream's encoder has produced its
    // first IDR frame.
    let decoder = new H264Decoder(streamId, onFrame);
    try {
        await decoder.init();
    } catch (e) {
        console.error("StreamLoop: Failed to initialize decoder:", e);
        return;
    }
    if (signal.aborted) { decoder.close(); return; }

    const INITIAL_DELAY_MS = 100;
    const MAX_DELAY_MS = 5000;
    let delay = INITIAL_DELAY_MS;

    // Outer reconnect loop.
    while (!signal.aborted) {
        try {
            const ws = await openWebSocket(`/api/streams/${streamId}`, signal);
            if (signal.aborted) break;

            delay = INITIAL_DELAY_MS; // Reset backoff on successful connect.

            // Inner message loop — process live-protocol messages until WS closes.
            for await (const data of wsMessages(ws, signal)) {
                const buf = new Uint8Array(data);
                if (buf.length < HEADER_SIZE) continue;

                const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
                const msgType = view.getUint8(0);
                const msgFlags = view.getUint8(1);

                if (msgType === MSG_FRAME) {
                    // Frame payload: [u64 LE: timestamp_us][avcc bytes]
                    const timestamp = Number(view.getBigUint64(HEADER_SIZE, true));
                    const isKeyframe = (msgFlags & FLAG_IS_KEYFRAME) !== 0;
                    const avcc = buf.subarray(HEADER_SIZE + 8);
                    decoder.decodeFrame(timestamp, isKeyframe, avcc);
                } else if (msgType === MSG_CODEC_PARAMS) {
                    // CodecParams: the encoder's SPS/PPS changed (rare — only on
                    // hot-swap in auto mode).  Reinitialize the decoder.
                    console.log("StreamLoop: CodecParams received, reinitializing decoder");
                    decoder.close();
                    decoder = new H264Decoder(streamId, onFrame);
                    while (!signal.aborted) {
                        try {
                            await decoder.init();
                            break;
                        } catch (e) {
                            console.warn("StreamLoop: Reinit failed, retrying:", e);
                            await sleep(1000);
                        }
                    }
                }
            }
        } catch {
            if (signal.aborted) break;
        }

        // Backoff before reconnecting.
        if (!signal.aborted) {
            console.log("StreamLoop: WS disconnected, reconnecting in %dms", delay);
            await sleep(delay);
            delay = Math.min(delay * 2, MAX_DELAY_MS);
        }
    }

    decoder.close();
    console.log("StreamLoop: Stream loop ended");
}

function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
