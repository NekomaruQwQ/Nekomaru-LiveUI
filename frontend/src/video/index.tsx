import { useEffect, useRef } from "react";

import { DEBUG } from "../../debug";
import { openWebSocket, wsMessages } from "../ws";
import { ChromaKeyRenderer, parseHexColor } from "./chroma-key";
import { H264Decoder } from "./decoder";

/**
 * Video renderer for a well-known stream ID ("main" or "youtube-music").
 *
 * The stream loop owns the full decoder lifecycle: it creates a decoder,
 * receives `live-protocol` framed messages via WebSocket, and decodes frames.
 * The server relays binary messages from the capture worker — each WS message
 * is a complete `live-protocol` frame (8-byte header + payload).
 *
 * 404 responses are treated as retriable (the server may create the stream
 * shortly), so the component can be rendered before the stream exists.
 *
 * When `chromaKey` is set (e.g. "#212121"), a WebGL2 fragment shader replaces
 * pixels matching that color with transparency.  The entire pipeline stays on
 * the GPU — no CPU readback.
 */
export function StreamRenderer({ streamId, chromaKey }: {
    streamId: string;
    chromaKey?: string;
}) {
    const canvasRef = useRef<HTMLCanvasElement>(null);

    useEffect(() => {
        console.log("StreamRenderer: Component mounted");

        const canvas = canvasRef.current;
        if (!canvas) {
            console.error("StreamRenderer: Canvas ref is null!");
            return;
        }

        // ── Build the frame renderer ─────────────────────────────────────
        // When chroma-key is active, use a WebGL2 shader that keys out the
        // target color.  Otherwise, use a plain 2D canvas drawImage path.
        let onFrame: (frame: VideoFrame) => void;
        let cleanup: (() => void) | undefined;

        if (chromaKey) {
            const renderer = new ChromaKeyRenderer(canvas, parseHexColor(chromaKey));
            onFrame = (frame) => renderer.render(frame);
            cleanup = () => renderer.dispose();
            console.log("StreamRenderer: Using WebGL chroma-key renderer (key=%s)", chromaKey);
        } else {
            const ctx = canvas.getContext("2d");
            if (!ctx) {
                console.error("StreamRenderer: Failed to get 2D context");
                return;
            }
            onFrame = (frame) => renderFrame(canvas, ctx, frame);
            console.log("StreamRenderer: Using 2D canvas renderer");
        }

        console.log("StreamRenderer: Canvas ready: %dx%d", canvas.width, canvas.height);

        const abortController = new AbortController();
        startStreamLoop(streamId, onFrame, abortController.signal);

        return () => {
            console.log("StreamRenderer: Component unmounting, aborting stream loop");
            abortController.abort();
            cleanup?.();
        };
    }, [streamId, chromaKey]);

    return (
        <canvas
            ref={canvasRef}
            className={`w-full object-contain ${chromaKey ? "" : "bg-[#1e1f22]"}`}
        />
    );
}

let lastFrameTime = 0;

/**
 * Render a decoded video frame to canvas.
 */
function renderFrame(canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D, frame: VideoFrame) {
    // Resize canvas if needed.
    if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
        canvas.width = frame.displayWidth;
        canvas.height = frame.displayHeight;
        console.log(
            "StreamRenderer: Canvas resized to %dx%d",
            frame.displayWidth,
            frame.displayHeight);
    }

    if (DEBUG.debugStreamRenderer) {
        console.log("StreamRenderer: Rendering frame to canvas - timestamp: %d μs", frame.timestamp);
    }
    ctx.drawImage(frame, 0, 0);

    // CRITICAL: Close frame to release GPU memory.
    frame.close();

    if (DEBUG.debugStreamRenderer) {
        console.log("StreamRenderer: Frame closed (GPU memory released)");
    }

    const now = performance.now();
    if (lastFrameTime > 0) {
        const delta = now - lastFrameTime;
        if (DEBUG.debugStreamRenderer) {
            console.log("StreamRenderer: Frame interval: %d ms", delta);
        }
    }
    lastFrameTime = now;
}

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
async function startStreamLoop(
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
            const ws = await openWebSocket(
                `/api/v1/ws/video/${streamId}`, signal);
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

// ── live-protocol constants ─────────────────────────────────────────────

/** Frame header size (bytes).  Matches `live-protocol` Rust crate. */
const HEADER_SIZE       = 8;
const MSG_CODEC_PARAMS  = 0x01;
const MSG_FRAME         = 0x02;
const FLAG_IS_KEYFRAME  = 1 << 0;
