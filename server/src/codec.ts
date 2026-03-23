/**
 * H.264 codec helpers for the /init endpoint.
 *
 * Ported from `live-protocol/src/avcc.rs`.  The server parses the cached
 * CodecParams payload to extract SPS/PPS and builds the codec string +
 * avcC descriptor that the frontend passes to `VideoDecoder.configure()`.
 */

import { HEADER_SIZE } from "./protocol";

/**
 * Parse a CodecParams payload from a raw message buffer.
 *
 * Payload layout (after 8-byte header):
 * ```
 * [u16 LE: width][u16 LE: height]
 * [u16 LE: sps_len][sps bytes]
 * [u16 LE: pps_len][pps bytes]
 * ```
 */
export function parseCodecParams(raw: Uint8Array): {
    width: number;
    height: number;
    sps: Uint8Array;
    pps: Uint8Array;
} {
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    let pos = HEADER_SIZE; // skip frame header

    const width  = view.getUint16(pos, true); pos += 2;
    const height = view.getUint16(pos, true); pos += 2;

    const spsLen = view.getUint16(pos, true); pos += 2;
    const sps = raw.subarray(pos, pos + spsLen); pos += spsLen;

    const ppsLen = view.getUint16(pos, true); pos += 2;
    const pps = raw.subarray(pos, pos + ppsLen);

    return { width, height, sps, pps };
}

/**
 * Strip the Annex B start code prefix from NAL data.
 *
 * Handles 4-byte (`00 00 00 01`) and 3-byte (`00 00 01`) forms.
 */
function stripStartCode(data: Uint8Array): Uint8Array {
    if (data.length >= 4 && data[0] === 0 && data[1] === 0 && data[2] === 0 && data[3] === 1) {
        return data.subarray(4);
    }
    if (data.length >= 3 && data[0] === 0 && data[1] === 0 && data[2] === 1) {
        return data.subarray(3);
    }
    return data;
}

/**
 * Build the `avc1.PPCCLL` codec string from SPS data.
 *
 * SPS layout (after stripping start code):
 * `[nal_header, profile_idc, constraint_flags, level_idc, ...]`
 */
export function buildCodecString(sps: Uint8Array): string {
    const s = stripStartCode(sps);
    if (s.length < 4) throw new Error(`SPS too short (${s.length} bytes)`);
    const hex = (b: number) => b.toString(16).padStart(2, "0");
    return `avc1.${hex(s[1])}${hex(s[2])}${hex(s[3])}`;
}

/**
 * Build an AVCDecoderConfigurationRecord (avcC) from SPS and PPS.
 *
 * ISO 14496-15 §5.2.4.1 — 11 + sps.length + pps.length bytes.
 */
export function buildAvccDescriptor(sps: Uint8Array, pps: Uint8Array): Uint8Array {
    const s = stripStartCode(sps);
    const p = stripStartCode(pps);
    if (s.length < 4) throw new Error(`SPS too short for avcC (${s.length} bytes)`);

    const buf = new Uint8Array(11 + s.length + p.length);
    const view = new DataView(buf.buffer);
    let pos = 0;

    buf[pos++] = 1;        // configurationVersion
    buf[pos++] = s[1];     // AVCProfileIndication
    buf[pos++] = s[2];     // profile_compatibility
    buf[pos++] = s[3];     // AVCLevelIndication
    buf[pos++] = 0xFF;     // lengthSizeMinusOne = 3
    buf[pos++] = 0xE1;     // numSPS = 1

    view.setUint16(pos, s.length, false); pos += 2; // BE
    buf.set(s, pos); pos += s.length;

    buf[pos++] = 1;        // numPPS
    view.setUint16(pos, p.length, false); pos += 2; // BE
    buf.set(p, pos);

    return buf;
}
