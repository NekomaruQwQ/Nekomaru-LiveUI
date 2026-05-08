<script lang="ts">
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

    import { onMount } from "svelte";
    import { runReconnectingWS } from "../ws";

    // ── live-protocol constants ──────────────────────────────────────────────

    /// Size of the live-protocol frame header in bytes.
    const HEADER_SIZE = 8;

    /// Message type discriminants (byte 0 of the frame header).
    const MSG_AUDIO_CONFIG = 0x11;
    const MSG_AUDIO_CHUNK = 0x12;

    onMount(() => {
        const abort = new AbortController();
        void startAudioLoop(abort.signal);
        return () => abort.abort();
    });

    // ── Audio loop ───────────────────────────────────────────────────────────

    /// AudioContext is reused across reconnects: it owns the speaker output
    /// and creating a fresh one per reconnect would briefly mute audio.  Only
    /// recreated when the server sends a different sample rate.
    async function startAudioLoop(signal: AbortSignal): Promise<void> {
        let ctx: AudioContext | null = null;

        await runReconnectingWS("/api/audio", signal, (ws) => new Promise<void>((resolve) => {
            let workletNode: AudioWorkletNode | null = null;
            let configReceived = false;

            ws.onmessage = async (ev: MessageEvent) => {
                if (!(ev.data instanceof ArrayBuffer)) return;
                const data = new Uint8Array(ev.data);
                if (data.length < HEADER_SIZE) return;

                const msgType = new DataView(
                    data.buffer, data.byteOffset, data.byteLength).getUint8(0);

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
                if (workletNode) {
                    workletNode.disconnect();
                    workletNode = null;
                }
                resolve();
            };

            ws.onerror = () => resolve();
        }));

        // Outer loop ended (signal aborted).  Tear down the AudioContext.
        if (ctx) {
            await (ctx as AudioContext).close();
            console.log("AudioStream: stopped");
        }
    }

    // ── Protocol parsers ─────────────────────────────────────────────────────

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
</script>
