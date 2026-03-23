/**
 * Hand-written constants matching `live-protocol` (Rust crate).
 *
 * The 8-byte frame header:
 * ```
 * Offset  Field            Size
 * 0       message_type     u8
 * 1       flags            u8
 * 2       reserved         u16 (zero)
 * 4       payload_length   u32 LE
 * ```
 */

export const HEADER_SIZE = 8;

export const enum MessageType {
    CodecParams = 0x01,
    Frame       = 0x02,
    KpmUpdate   = 0x10,
    Error       = 0xFF,
}

export const enum Flags {
    IS_KEYFRAME = 1 << 0,
}

/** Read the message type from a raw message buffer (byte 0). */
export function getMessageType(buf: ArrayBuffer | Uint8Array): number {
    const view = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
    return view[0];
}

/** Read the flags byte from a raw message buffer (byte 1). */
export function getFlags(buf: ArrayBuffer | Uint8Array): number {
    const view = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
    return view[1];
}
