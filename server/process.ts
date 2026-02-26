// Process manager for live-capture.exe instances.
//
// Each capture stream is a child process that writes binary IPC messages to
// stdout.  This module spawns them, wires stdout through the ProtocolParser
// into a StreamBuffer, and handles lifecycle events.

import type { Subprocess } from "bun";

import { StreamBuffer } from "./buffer";
import { captureExePath, frameBufferCapacity } from "./common";
import { ProtocolParser } from "./protocol";

// ── Types ────────────────────────────────────────────────────────────────────

export interface CaptureStream {
    id: string;
    hwnd: string;
    status: "starting" | "running" | "stopped";
    buffer: StreamBuffer;
    /// Bun child process handle.  Null after the process has exited and been
    /// cleaned up by destroyStream().
    process: Subprocess | null;
}

// ── Registry ─────────────────────────────────────────────────────────────────

const streams = new Map<string, CaptureStream>();

export function getStream(id: string): CaptureStream | undefined {
    return streams.get(id);
}

export function listStreams(): CaptureStream[] {
    return [...streams.values()];
}

// ── Enumerate windows ────────────────────────────────────────────────────────

/// Spawn `live-capture.exe --enumerate-windows` and return the JSON array
/// of capturable windows.  This is a one-shot process, not a long-running
/// capture.
export async function enumerateWindows(): Promise<unknown[]> {
    const proc = Bun.spawn([captureExePath, "--enumerate-windows"], {
        stdout: "pipe",
        stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    await proc.exited;

    return JSON.parse(stdout) as unknown[];
}

// ── Create / destroy streams ─────────────────────────────────────────────────

/// Spawn a live-capture.exe child process with the given CLI args, wire up
/// stdout parsing and stderr forwarding, and register the stream.
///
/// Returns immediately; status transitions to "running" once the first
/// CodecParams message arrives from the encoder.
function spawnCapture(hwnd: string, args: string[], label: string): CaptureStream {
    const id = crypto.randomUUID().slice(0, 8);
    const buffer = new StreamBuffer(frameBufferCapacity);

    const proc = Bun.spawn(args, { stdout: "pipe", stderr: "pipe" });

    const stream: CaptureStream = {
        id, hwnd,
        status: "starting",
        buffer,
        process: proc,
    };

    streams.set(id, stream);

    // Wire stdout → ProtocolParser → StreamBuffer
    const parser = new ProtocolParser((msg) => {
        switch (msg.type) {
            case "codec_params":
                stream.buffer.setCodecParams(msg.params);
                if (stream.status === "starting") {
                    stream.status = "running";
                    console.log(`[stream:${id}] running (codec params received)`);
                }
                break;
            case "frame":
                stream.buffer.pushFrame(msg.frame);
                break;
            case "error":
                console.error(`[stream:${id}] capture error: ${msg.message}`);
                break;
        }
    });

    pipeStdout(id, proc, parser);
    pipeStderr(id, proc);

    proc.exited.then((code) => {
        console.log(`[stream:${id}] process exited with code ${code}`);
        stream.status = "stopped";
    });

    console.log(`[stream:${id}] spawned (${label})`);
    return stream;
}

/// Spawn a resample-mode capture: scales the full window to `width x height`.
export function createStream(hwnd: string, width: number, height: number): CaptureStream {
    return spawnCapture(hwnd,
        [captureExePath, "--hwnd", hwnd, "--width", String(width), "--height", String(height)],
        `hwnd=${hwnd}, resample ${width}x${height}`);
}

/// Spawn a crop-mode capture: extracts a subrect at native resolution.
///
/// `cropWidth`/`cropHeight` are either a pixel count (multiple of 16) or
/// `"full"` for the source dimension.  `cropAlign` controls placement of the
/// crop rect within the source window (e.g. "bottom", "center").
export function createCropStream(
    hwnd: string,
    cropWidth: "full" | number,
    cropHeight: "full" | number,
    cropAlign: string): CaptureStream {
    return spawnCapture(hwnd,
        [captureExePath, "--hwnd", hwnd,
            "--crop-width", String(cropWidth),
            "--crop-height", String(cropHeight),
            "--crop-align", cropAlign],
        `hwnd=${hwnd}, crop ${cropWidth}x${cropHeight} ${cropAlign}`);
}

/// Kill the child process and remove the stream from the registry.
export function destroyStream(id: string): void {
    const stream = streams.get(id);
    if (!stream) return;

    if (stream.process) {
        stream.process.kill();
        stream.process = null;
    }

    streams.delete(id);
    console.log(`[stream:${id}] destroyed`);
}

/// Kill all child processes.  Called on server shutdown.
export function destroyAll(): void {
    for (const [id] of streams) {
        destroyStream(id);
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Read stdout chunks from the child process and feed them to the parser.
async function pipeStdout(id: string, proc: Subprocess, parser: ProtocolParser): Promise<void> {
    try {
        const reader = proc.stdout.getReader();
        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            parser.feed(value);
        }
    } catch (e) {
        // Expected when the process is killed — the stream closes.
        console.log(`[stream:${id}] stdout closed`);
    }
}

/// Forward stderr lines with a prefix for easy identification in the console.
async function pipeStderr(id: string, proc: Subprocess): Promise<void> {
    try {
        const reader = proc.stderr.getReader();
        const decoder = new TextDecoder();
        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            process.stderr.write(`[capture:${id}] ${decoder.decode(value)}`);
        }
    } catch {
        // Expected on process kill.
    }
}
