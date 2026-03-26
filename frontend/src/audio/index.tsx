// Audio streaming component.
//
// Invisible component that connects to /api/audio via WebSocket and plays
// PCM audio through an AudioWorklet.  Mounts at the app root — audio is
// global (not per-stream).
//
// Protocol: receives raw live-protocol framed binary messages via WS.
// The first message is AudioConfig (byte 0 = 0x11) with format params.
// Subsequent messages are AudioChunk (byte 0 = 0x12) with PCM data.
//
// No A/V sync — both audio and video use wall-clock timestamps from the
// same machine, and the ~20ms latency difference (audio has no encoding
// step) is imperceptible.  Chunks are posted to the worklet immediately.

import { useEffect } from "react";

// ── live-protocol constants ──────────────────────────────────────────────────

/// Size of the live-protocol frame header in bytes.
const HEADER_SIZE = 8;

/// Message type discriminants (byte 0 of the frame header).
const MSG_AUDIO_CONFIG = 0x11;
const MSG_AUDIO_CHUNK = 0x12;

// ── Reconnect constants ──────────────────────────────────────────────────────

const INITIAL_DELAY_MS = 100;
const MAX_DELAY_MS = 5000;

// ── Component ────────────────────────────────────────────────────────────────

/// Invisible component that streams audio from the server.
/// Renders nothing — audio output goes to AudioContext.destination.
export function AudioStream() {
    useEffect(() => {
        const abort = new AbortController();
        startAudioLoop(abort.signal);
        return () => abort.abort();
    }, []);

    return null;
}

// ── Audio loop ───────────────────────────────────────────────────────────────

async function startAudioLoop(signal: AbortSignal): Promise<void> {
    let delay = INITIAL_DELAY_MS;
    let ctx: AudioContext | null = null;

    while (!signal.aborted) {
        try {
            const connected = await runOneAudioConnection(signal, ctx, (newCtx) => {
                ctx = newCtx;
            });
            if (connected) delay = INITIAL_DELAY_MS;
        } catch {
            // Connection failed — fall through to backoff.
        }

        if (signal.aborted) break;
        await sleep(delay);
        delay = Math.min(delay * 2, MAX_DELAY_MS);
    }

    // Cleanup.
    if (ctx) {
        await (ctx as AudioContext).close();
        console.log("AudioStream: stopped");
    }
}

/// Run a single WS connection to /api/audio.  Returns true if the connection
/// was successfully established (for backoff reset).
async function runOneAudioConnection(
    signal: AbortSignal,
    existingCtx: AudioContext | null,
    setCtx: (ctx: AudioContext) => void,
): Promise<boolean> {
    return new Promise<boolean>((resolve, reject) => {
        if (signal.aborted) { resolve(false); return; }

        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(`${proto}//${location.host}/api/audio`);
        ws.binaryType = "arraybuffer";

        const onAbort = () => ws.close();
        signal.addEventListener("abort", onAbort, { once: true });

        let ctx = existingCtx;
        let workletNode: AudioWorkletNode | null = null;
        let configReceived = false;

        ws.onmessage = async (ev: MessageEvent) => {
            if (!(ev.data instanceof ArrayBuffer)) return;
            const data = new Uint8Array(ev.data);
            if (data.length < HEADER_SIZE) return;

            // biome-ignore lint/style/noNonNullAssertion: length check above
            const msgType = data[0]!;

            if (msgType === MSG_AUDIO_CONFIG && !configReceived) {
                configReceived = true;
                const config = parseAudioConfig(data);
                if (!config) return;

                console.log("AudioStream: config %dHz %dch %d-bit",
                    config.sampleRate, config.channels, config.bitsPerSample);

                try {
                    // Create or reuse AudioContext at the device's sample rate.
                    if (!ctx || ctx.sampleRate !== config.sampleRate) {
                        if (ctx) await ctx.close();
                        ctx = new AudioContext({ sampleRate: config.sampleRate });
                        setCtx(ctx);
                    }

                    // Handle browser autoplay policy.
                    if (ctx.state === "suspended") {
                        const resume = () => {
                            ctx?.resume();
                            document.removeEventListener("click", resume);
                            document.removeEventListener("keydown", resume);
                        };
                        document.addEventListener("click", resume, { once: true });
                        document.addEventListener("keydown", resume, { once: true });
                    }

                    // Load worklet module.
                    await ctx.audioWorklet.addModule(
                        new URL("./worklet.ts", import.meta.url));

                    workletNode = new AudioWorkletNode(ctx, "pcm-worklet-processor", {
                        outputChannelCount: [config.channels],
                    });
                    workletNode.connect(ctx.destination);
                } catch (e) {
                    console.error("AudioStream: worklet setup failed:", e);
                    ws.close();
                }
            } else if (msgType === MSG_AUDIO_CHUNK && workletNode) {
                const pcmData = parseAudioChunk(data);
                if (pcmData) {
                    const samples = new Int16Array(
                        pcmData.buffer, pcmData.byteOffset, pcmData.byteLength / 2);
                    workletNode.port.postMessage({
                        type: "pcm",
                        samples,
                        channels: workletNode.channelCount,
                    });
                }
            }
        };

        ws.onclose = () => {
            signal.removeEventListener("abort", onAbort);
            if (workletNode) {
                workletNode.disconnect();
                workletNode = null;
            }
            resolve(true);
        };

        ws.onerror = () => {
            signal.removeEventListener("abort", onAbort);
            reject(new Error("WebSocket error"));
        };
    });
}

// ── Protocol parsers ─────────────────────────────────────────────────────────

interface AudioConfigData {
    sampleRate: number;
    channels: number;
    bitsPerSample: number;
}

/// Parse an AudioConfig message (header + payload).
/// Payload: [u32 LE: sample_rate][u8: channels][u8: bits_per_sample][u16: reserved]
function parseAudioConfig(raw: Uint8Array): AudioConfigData | null {
    if (raw.length < HEADER_SIZE + 6) return null;
    const view = new DataView(raw.buffer, raw.byteOffset + HEADER_SIZE);
    return {
        sampleRate: view.getUint32(0, true),
        channels: view.getUint8(4),
        bitsPerSample: view.getUint8(5),
    };
}

/// Parse an AudioChunk message (header + payload).
/// Payload: [u64 LE: timestamp_us][PCM bytes]
/// Returns just the PCM data portion (skipping the 8-byte timestamp).
function parseAudioChunk(raw: Uint8Array): Uint8Array | null {
    // Header (8) + timestamp (8) + at least some PCM data.
    if (raw.length <= HEADER_SIZE + 8) return null;
    return raw.subarray(HEADER_SIZE + 8);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
