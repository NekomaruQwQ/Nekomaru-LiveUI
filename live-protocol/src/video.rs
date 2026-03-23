//! Video-specific payload serialization and deserialization.
//!
//! These functions serialize/deserialize the *payload* portion of video
//! messages.  The 8-byte frame header is handled by the parent module.

use std::io;

// ── CodecParams Payload ─────────────────────────────────────────────────────

/// Codec parameters extracted from the H.264 stream (SPS + PPS + resolution).
///
/// Payload layout:
/// ```text
/// [u16 LE: width][u16 LE: height]
/// [u16 LE: sps_length][sps bytes]
/// [u16 LE: pps_length][pps bytes]
/// ```
#[derive(Debug, Clone)]
pub struct CodecParams {
    /// Sequence Parameter Set (raw NAL data, may include Annex B start code).
    pub sps: Vec<u8>,
    /// Picture Parameter Set (raw NAL data, may include Annex B start code).
    pub pps: Vec<u8>,
    /// Video width in pixels.
    pub width: u32,
    /// Video height in pixels.
    pub height: u32,
}

/// Serialize `CodecParams` into the payload byte format.
pub fn write_codec_params_payload(params: &CodecParams) -> Vec<u8> {
    let len = 2 + 2 + 2 + params.sps.len() + 2 + params.pps.len();
    let mut buf = Vec::with_capacity(len);

    buf.extend_from_slice(&(params.width as u16).to_le_bytes());
    buf.extend_from_slice(&(params.height as u16).to_le_bytes());
    buf.extend_from_slice(&(params.sps.len() as u16).to_le_bytes());
    buf.extend_from_slice(&params.sps);
    buf.extend_from_slice(&(params.pps.len() as u16).to_le_bytes());
    buf.extend_from_slice(&params.pps);

    buf
}

/// Deserialize `CodecParams` from a payload byte slice.
pub fn read_codec_params_payload(data: &[u8]) -> io::Result<CodecParams> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "truncated CodecParams payload");
    if data.len() < 8 { return Err(invalid()); }

    let mut pos = 0;

    let width = u16::from_le_bytes([data[pos], data[pos + 1]]) as u32;
    pos += 2;
    let height = u16::from_le_bytes([data[pos], data[pos + 1]]) as u32;
    pos += 2;

    let sps_len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;
    if pos + sps_len > data.len() { return Err(invalid()); }
    let sps = data[pos..pos + sps_len].to_vec();
    pos += sps_len;

    if pos + 2 > data.len() { return Err(invalid()); }
    let pps_len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;
    if pos + pps_len > data.len() { return Err(invalid()); }
    let pps = data[pos..pos + pps_len].to_vec();

    Ok(CodecParams { sps, pps, width, height })
}

// ── Frame Payload ───────────────────────────────────────────────────────────

/// Serialize a video frame payload.
///
/// Payload layout:
/// ```text
/// [u64 LE: timestamp_us][avcc bytes]
/// ```
///
/// Note: `is_keyframe` is NOT in the payload — it lives in the frame header's
/// `flags` field (`flags::IS_KEYFRAME`).
pub fn write_frame_payload(timestamp_us: u64, avcc_data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + avcc_data.len());
    buf.extend_from_slice(&timestamp_us.to_le_bytes());
    buf.extend_from_slice(avcc_data);
    buf
}

/// Parse a video frame payload.  Returns `(timestamp_us, avcc_data)`.
///
/// The `is_keyframe` flag is in the frame header, not in the payload.
pub fn read_frame_payload(data: &[u8]) -> io::Result<(u64, &[u8])> {
    if data.len() < 8 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated Frame payload"));
    }
    let timestamp_us = u64::from_le_bytes(data[..8].try_into().unwrap());
    Ok((timestamp_us, &data[8..]))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_params_roundtrip() {
        let params = CodecParams {
            sps: vec![0x67, 0x42, 0xC0, 0x1E, 0xD9, 0x00, 0xA0, 0x47, 0xFE, 0xC8],
            pps: vec![0x68, 0xCE, 0x38, 0x80],
            width: 1920,
            height: 1200,
        };

        let payload = write_codec_params_payload(&params);
        let decoded = read_codec_params_payload(&payload).unwrap();

        assert_eq!(decoded.width, 1920);
        assert_eq!(decoded.height, 1200);
        assert_eq!(decoded.sps, params.sps);
        assert_eq!(decoded.pps, params.pps);
    }

    #[test]
    fn frame_payload_roundtrip() {
        let avcc = vec![0x00, 0x00, 0x00, 0x05, 0x65, 0x88, 0x80, 0x40, 0x00];
        let payload = write_frame_payload(16_667, &avcc);

        let (ts, data) = read_frame_payload(&payload).unwrap();
        assert_eq!(ts, 16_667);
        assert_eq!(data, avcc);
    }

    #[test]
    fn frame_payload_empty_avcc() {
        let payload = write_frame_payload(0, &[]);
        let (ts, data) = read_frame_payload(&payload).unwrap();
        assert_eq!(ts, 0);
        assert!(data.is_empty());
    }
}
