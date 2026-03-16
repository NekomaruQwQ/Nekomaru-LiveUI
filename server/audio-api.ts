// HTTP API routes for audio streaming.
//
// Mounted at /api/v1/audio in index.ts.  All routes are relative to that base:
//   GET /init          → audio format params (sample rate, channels, bit depth)
//   GET /chunks?after=N → binary audio chunks since sequence N

import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";

import { audioManager } from "./audio";

const audioApi = new Hono()

    /// Return audio format parameters for the frontend's AudioContext setup.
    /// Returns 503 if the capture process hasn't sent params yet.
    .get("/init", (c) => {
        if (!audioManager.active) return c.json({ error: "audio disabled" }, 404);
        const params = audioManager.buffer.getAudioParams();
        if (!params) return c.json({ error: "audio params not yet available" }, 503);

        return c.json({
            sampleRate: params.sampleRate,
            channels: params.channels,
            bitsPerSample: params.bitsPerSample,
        });
    })

    /// Return audio chunks after a given sequence number as a binary blob.
    /// The frontend polls this endpoint at ~16ms intervals with ?after=lastSeq.
    ///
    /// Binary layout (all little-endian):
    ///   [u32: num_chunks]
    ///   per chunk: [u32: sequence][u32: payload_length][payload bytes]
    ///
    /// Payload per chunk: [u64 LE: timestamp_us][delta-encoded s16le PCM bytes]
    ///
    /// Delta encoding: first sample stored as-is, subsequent samples are
    /// (current - previous).  Resets per chunk (no cross-chunk state).
    /// The full response is then gzip-compressed (Content-Encoding: gzip).
    .get("/chunks",
        zValidator("query", z.object({ after: z.string().optional() })),
        (c) => {
            const after = parseInt(c.req.valid("query").after ?? "0", 10) || 0;
            const chunks = audioManager.buffer.getChunksAfter(after);

            // Pre-compute total size: 4-byte header + (8 + payload) per chunk.
            let totalSize = 4;
            for (const ch of chunks) totalSize += 8 + ch.payload.length;

            const buf = new Uint8Array(totalSize);
            const view = new DataView(buf.buffer);
            let pos = 0;

            // Header: chunk count.
            view.setUint32(pos, chunks.length, true); pos += 4;

            // Each chunk: sequence + payload length + delta-encoded payload.
            for (const ch of chunks) {
                view.setUint32(pos, ch.sequence, true);       pos += 4;
                view.setUint32(pos, ch.payload.length, true); pos += 4;

                // Copy payload — must not mutate the buffer's shared data.
                buf.set(ch.payload, pos);

                // Delta-encode the PCM region in-place on the copy.
                // Payload layout: [u64 timestamp (8 bytes)][s16le PCM samples...].
                deltaEncodePcm(buf, pos + 8, ch.payload.length - 8);

                pos += ch.payload.length;
            }

            const compressed = Bun.gzipSync(buf);
            return c.body(compressed, 200, {
                "Content-Type": "application/octet-stream",
                "Content-Encoding": "gzip",
            });
        });

export type AudioApiType = typeof audioApi;
export default audioApi;

// ── Delta encoding ──────────────────────────────────────────────────────────

/// Delta-encode s16le PCM samples in-place within `buf`.
/// First sample is kept as-is; each subsequent sample becomes (current - prev).
/// Iterates backwards so earlier values aren't clobbered before they're read.
function deltaEncodePcm(buf: Uint8Array, byteOffset: number, byteLength: number): void {
    const view = new DataView(buf.buffer, buf.byteOffset);
    const sampleCount = byteLength >>> 1; // 2 bytes per s16 sample
    // Walk backwards: delta[i] = sample[i] - sample[i-1].
    for (let i = sampleCount - 1; i >= 1; i--) {
        const off = byteOffset + i * 2;
        const cur = view.getInt16(off, true);
        const prev = view.getInt16(off - 2, true);
        view.setInt16(off, (cur - prev) | 0, true);
    }
}
