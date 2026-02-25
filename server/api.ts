// HTTP API routes for stream management.
//
// Mounted at /streams in index.ts.  All routes are relative to that base:
//   GET  /              → list streams
//   POST /              → create a new capture
//   DELETE /:id         → destroy a capture
//   GET  /:id/init      → codec params (SPS/PPS/resolution)
//   GET  /:id/frames    → encoded frames (polling)
//   GET  /windows       → enumerate capturable windows

import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";

import * as proc from "@/process";

const api = new Hono();

// ── Stream management ────────────────────────────────────────────────────────

/// List all active capture streams.
api.get("/", (c) => {
    const streams = proc.listStreams();
    return c.json(streams.map((s) => ({
        id: s.id,
        hwnd: s.hwnd,
        width: s.width,
        height: s.height,
        status: s.status,
    })));
});

/// Create a new capture stream (spawns a live-capture.exe instance).
const createSchema = z.object({
    hwnd: z.string(),
    width: z.number().int().positive(),
    height: z.number().int().positive(),
});

api.post("/", zValidator("json", createSchema), (c) => {
    const { hwnd, width, height } = c.req.valid("json");
    const stream = proc.createStream(hwnd, width, height);
    return c.json({ id: stream.id }, 201);
});

/// Destroy a capture stream (kills the child process).
api.delete("/:id", (c) => {
    const id = c.req.param("id");
    const stream = proc.getStream(id);
    if (!stream) return c.json({ error: "stream not found" }, 404);
    proc.destroyStream(id);
    return c.json({ ok: true });
});

// ── Stream data ──────────────────────────────────────────────────────────────

/// Return codec initialization parameters for the decoder.
/// Returns 503 if the encoder hasn't produced its first IDR frame yet —
/// the frontend has retry logic and will poll again.
api.get("/:id/init", (c) => {
    const stream = proc.getStream(c.req.param("id"));
    if (!stream) return c.json({ error: "stream not found" }, 404);

    const params = stream.buffer.getCodecParams();
    if (!params) return c.json({ error: "codec params not yet available" }, 503);

    return c.json({
        sps: uint8ToBase64(params.sps),
        pps: uint8ToBase64(params.pps),
        width: params.width,
        height: params.height,
    });
});

/// Return encoded frames after a given sequence number.
/// The frontend polls this endpoint at ~60fps with ?after=lastSequence.
api.get("/:id/frames", (c) => {
    const stream = proc.getStream(c.req.param("id"));
    if (!stream) return c.json({ error: "stream not found" }, 404);

    const after = parseInt(c.req.query("after") ?? "0", 10) || 0;
    const frames = stream.buffer.getFramesAfter(after);

    return c.json({
        frames: frames.map((f) => ({
            sequence: f.sequence,
            data: uint8ToBase64(f.payload),
        })),
    });
});

// ── Window enumeration ───────────────────────────────────────────────────────

/// List capturable windows.  One-shot spawn of live-capture.exe --enumerate-windows.
api.get("/windows", async (c) => {
    const windows = await proc.enumerateWindows();
    return c.json(windows);
});

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Encode a Uint8Array to standard base64.
/// The frontend decodes with Uint8Array.fromBase64() (TC39 Stage 3, Chrome 117+).
function uint8ToBase64(data: Uint8Array): string {
    return Buffer.from(data).toString("base64");
}

export default api;
