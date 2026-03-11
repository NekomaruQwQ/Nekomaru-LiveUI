// Incremental binary parser for the live-capture IPC wire protocol.
//
// live-capture.exe writes length-prefixed binary messages to stdout.
// Node.js delivers stdout data in arbitrary-sized chunks, so this parser
// accumulates bytes internally and emits complete messages via a callback.
//
// Wire format per message:
//   [u8:  message_type]
//   [u32 LE: payload_length]
//   [payload_length bytes: payload]
//
// See service/capture/src/lib.rs for the authoritative Rust definition.

// ── Types ────────────────────────────────────────────────────────────────────

/// Mirrors Rust NALUnitType (service/capture/src/lib.rs).
export const NALUnitType = {
    NonIDR: 1,
    IDR:    5,
    SPS:    7,
    PPS:    8,
} as const;

export interface NALUnit {
    /// Raw nal_unit_type byte from the wire.
    unitType: number;
    /// Raw NAL data (includes Annex B start code).
    data: Uint8Array;
}

/// Codec initialization parameters (SPS, PPS, resolution).
/// Sent once after encoder init and again if SPS/PPS change.
export interface CodecParams {
    width: number;
    height: number;
    /// Raw SPS bytes (without Annex B start code).
    sps: Uint8Array;
    /// Raw PPS bytes (without Annex B start code).
    pps: Uint8Array;
}

/// One encoded video frame with its NAL units.
export interface FrameMessage {
    /// Timestamp in microseconds (u64 from wire, kept as bigint to avoid precision loss).
    timestampUs: bigint;
    /// Whether this frame contains an IDR NAL unit.
    isKeyframe: boolean;
    /// All NAL units that make up this frame.
    nalUnits: NALUnit[];
}

/// A parsed IPC message from the capture process.
export type IpcMessage =
    | { type: "codec_params"; params: CodecParams }
    | { type: "frame"; frame: FrameMessage }
    | { type: "error"; message: string };

// ── Message type discriminants ───────────────────────────────────────────────

import { createStreamLogger } from "./log";

const MSG_CODEC_PARAMS = 0x01;
const MSG_FRAME        = 0x02;
const MSG_ERROR        = 0xFF;

/// Minimum header size: type(1) + payload_length(4).
const HEADER_SIZE = 5;

// ── Parser ───────────────────────────────────────────────────────────────────

/// Push-based incremental parser for the binary IPC protocol.
///
/// Call `feed(chunk)` each time new data arrives from stdout.
/// Complete messages are emitted via the callback passed to the constructor.
export class ProtocolParser {
    /// Internal accumulation buffer.  Grows as data arrives, shrinks as
    /// complete messages are consumed.
    private buffer = new Uint8Array(0);
    private callback: (msg: IpcMessage) => void;
    private streamId: string;

    constructor(streamId: string, callback: (msg: IpcMessage) => void) {
        this.streamId = streamId;
        this.callback = callback;
    }

    /// Append new data from stdout and parse all complete messages.
    feed(chunk: Uint8Array): void {
        this.buffer = concatUint8(this.buffer, chunk);

        // Greedy parse loop: consume as many complete messages as possible.
        // A single stdout chunk may contain multiple frames (common when the
        // capture process is faster than the event loop).
        while (true) {
            if (this.buffer.length < HEADER_SIZE) break;

            const view = new DataView(
                this.buffer.buffer,
                this.buffer.byteOffset,
                this.buffer.byteLength);

            const payloadLength = view.getUint32(1, /* littleEndian */ true);
            const totalLength = HEADER_SIZE + payloadLength;

            // Not enough data for a complete message yet — wait for more.
            if (this.buffer.length < totalLength) break;

            const messageType = this.buffer[0]!;
            const payload = this.buffer.slice(HEADER_SIZE, totalLength);

            // Advance past the consumed message.
            this.buffer = this.buffer.subarray(totalLength);

            const msg = parsePayload(this.streamId, messageType, payload);
            if (msg) this.callback(msg);
        }
    }
}

// ── Payload parsers ──────────────────────────────────────────────────────────

function parsePayload(streamId: string, type: number, payload: Uint8Array): IpcMessage | null {
    switch (type) {
        case MSG_CODEC_PARAMS: return parseCodecParams(payload);
        case MSG_FRAME:        return parseFrame(payload);
        case MSG_ERROR:        return parseError(payload);
        default:
            createStreamLogger(streamId, "server::protocol").error(`unknown message type 0x${type.toString(16)}`);
            return null;
    }
}

/// Parse a CodecParams payload.
///
/// Wire layout:
///   [u16 LE: width][u16 LE: height]
///   [u16 LE: sps_length][sps bytes]
///   [u16 LE: pps_length][pps bytes]
function parseCodecParams(data: Uint8Array): IpcMessage {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let pos = 0;

    const width = view.getUint16(pos, true);  pos += 2;
    const height = view.getUint16(pos, true); pos += 2;

    const spsLen = view.getUint16(pos, true); pos += 2;
    const sps = data.slice(pos, pos + spsLen); pos += spsLen;

    const ppsLen = view.getUint16(pos, true); pos += 2;
    const pps = data.slice(pos, pos + ppsLen);

    return { type: "codec_params", params: { width, height, sps, pps } };
}

/// Parse a Frame payload.
///
/// Wire layout:
///   [u64 LE: timestamp_us][u8: is_keyframe]
///   [u32 LE: num_nal_units]
///   for each NAL: [u8: nal_type][u32 LE: data_length][data bytes]
function parseFrame(data: Uint8Array): IpcMessage {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    let pos = 0;

    const timestampUs = view.getBigUint64(pos, true); pos += 8;
    const isKeyframe = data[pos]! !== 0;               pos += 1;
    const numNals = view.getUint32(pos, true);         pos += 4;

    const nalUnits: NALUnit[] = [];
    for (let i = 0; i < numNals; i++) {
        const unitType = data[pos]!;                          pos += 1;
        const dataLen = view.getUint32(pos, true);            pos += 4;
        const nalData = data.slice(pos, pos + dataLen);       pos += dataLen;
        nalUnits.push({ unitType, data: nalData });
    }

    return { type: "frame", frame: { timestampUs, isKeyframe, nalUnits } };
}

/// Parse an Error payload (raw UTF-8).
function parseError(data: Uint8Array): IpcMessage {
    const message = new TextDecoder().decode(data);
    return { type: "error", message };
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Concatenate two Uint8Arrays into a new one.
function concatUint8(a: Uint8Array, b: Uint8Array): Uint8Array {
    if (a.length === 0) return b;
    const result = new Uint8Array(a.length + b.length);
    result.set(a, 0);
    result.set(b, a.length);
    return result;
}
