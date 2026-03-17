//! Per-stream circular frame buffer with SPS/PPS cache.
//!
//! Frames are pre-serialized on push into AVCC format — concatenated
//! length-prefixed NAL units ready for the browser's `EncodedVideoChunk`:
//!
//! ```text
//! [u32 BE: nal_length][raw NAL data (no start code)]
//! [u32 BE: nal_length][raw NAL data (no start code)]
//! ...
//! ```
//!
//! The Annex B → AVCC conversion happens once at push time so HTTP
//! responses can serve the payload with zero per-request work.

use live_video::{CodecParams, FrameMessage};

// ── Types ────────────────────────────────────────────────────────────────────

/// A buffered frame with its pre-serialized AVCC payload.
pub struct BufferedFrame {
    /// Monotonically increasing sequence number (starts at 1).
    pub sequence: u32,
    /// Whether this frame contains an IDR NAL unit.
    pub is_keyframe: bool,
    /// Wall-clock timestamp in microseconds (promoted to the HTTP envelope so
    /// the frontend doesn't need to parse the payload).
    pub timestamp_us: u64,
    /// Pre-serialized AVCC payload — length-prefixed NAL units, directly
    /// feedable to `EncodedVideoChunk.data`.
    pub payload: Vec<u8>,
}

// ── StreamBuffer ─────────────────────────────────────────────────────────────

/// Fixed-capacity circular buffer for encoded video frames.
///
/// Multiple HTTP clients can read concurrently without draining — reads use
/// sequence-based filtering, not pop semantics.
pub struct StreamBuffer {
    frames: Vec<Option<BufferedFrame>>,
    capacity: usize,
    write_index: usize,
    count: usize,
    next_sequence: u32,
    codec_params: Option<CodecParams>,
}

impl StreamBuffer {
    pub fn new(capacity: usize) -> Self {
        let mut frames = Vec::with_capacity(capacity);
        frames.resize_with(capacity, || None);
        Self {
            frames,
            capacity,
            write_index: 0,
            count: 0,
            next_sequence: 1,
            codec_params: None,
        }
    }

    /// Cache the latest codec parameters (SPS/PPS/resolution).
    pub fn set_codec_params(&mut self, params: CodecParams) {
        self.codec_params = Some(params);
    }

    /// Return the cached codec params, or `None` if the encoder hasn't
    /// produced its first IDR frame yet.
    pub const fn get_codec_params(&self) -> Option<&CodecParams> {
        self.codec_params.as_ref()
    }

    /// Clear all buffered state — frames, codec params, and sequence counter.
    /// Called when the underlying capture process is replaced so stale frames
    /// from the old process are never served.
    pub fn reset(&mut self) {
        for slot in &mut self.frames { *slot = None; }
        self.write_index = 0;
        self.count = 0;
        self.next_sequence = 1;
        self.codec_params = None;
    }

    /// Push a parsed frame into the circular buffer.
    ///
    /// Assigns the next sequence number and pre-serializes the frame payload
    /// so HTTP responses don't need to re-serialize on every request.
    pub fn push_frame(&mut self, frame: &FrameMessage) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let payload = serialize_avcc_payload(frame);
        let idx = self.write_index % self.capacity;
        self.frames[idx] = Some(BufferedFrame {
            sequence,
            is_keyframe: frame.is_keyframe,
            timestamp_us: frame.timestamp_us,
            payload,
        });

        self.write_index += 1;
        if self.count < self.capacity { self.count += 1; }
    }

    /// Return all buffered frames with `sequence > after_sequence`.
    ///
    /// When `after_sequence` is 0 (first request from a new client),
    /// non-keyframes are skipped until the first keyframe is found — the
    /// WebCodecs decoder needs an IDR frame to initialize.
    pub fn get_frames_after(&self, after_sequence: u32) -> Vec<&BufferedFrame> {
        let mut result = Vec::new();
        let start = self.write_index.wrapping_sub(self.count);
        let mut need_keyframe = after_sequence == 0;

        for i in 0..self.count {
            let raw_idx = start.wrapping_add(i);
            let idx = raw_idx % self.capacity;

            let &Some(ref frame) = &self.frames[idx] else { continue };
            if frame.sequence <= after_sequence { continue; }

            if need_keyframe {
                if !frame.is_keyframe { continue; }
                need_keyframe = false;
            }

            result.push(frame);
        }

        result
    }
}

// ── Annex B → AVCC ──────────────────────────────────────────────────────────

/// Strip the Annex B start code prefix from a NAL unit.
///
/// Media Foundation produces both 4-byte (`00 00 00 01`) and 3-byte
/// (`00 00 01`) forms.  Returns the raw NAL data after the prefix.
fn strip_start_code(data: &[u8]) -> &[u8] {
    if data.get(..4) == Some(&[0, 0, 0, 1]) { return &data[4..]; }
    if data.get(..3) == Some(&[0, 0, 1])     { return &data[3..]; }
    data
}

/// Serialize a `FrameMessage` into AVCC format for `EncodedVideoChunk`.
///
/// Layout — concatenated length-prefixed NAL units:
/// ```text
/// [u32 BE: nal_length][raw NAL data]
/// [u32 BE: nal_length][raw NAL data]
/// ...
/// ```
///
/// Annex B start codes are stripped; each NAL gets a 4-byte big-endian
/// length prefix per ISO 14496-15.
fn serialize_avcc_payload(frame: &FrameMessage) -> Vec<u8> {
    let stripped: Vec<&[u8]> = frame.nal_units.iter()
        .map(|nal| strip_start_code(&nal.data))
        .collect();
    let total: usize = stripped.iter().map(|d| 4 + d.len()).sum();

    let mut buf = Vec::with_capacity(total);
    for data in &stripped {
        buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
        buf.extend_from_slice(data);
    }
    buf
}

// ── Codec helpers (used by the /init endpoint) ──────────────────────────────

/// Build the `avc1.PPCCLL` codec string from cached SPS.
///
/// SPS layout (after stripping any Annex B start code):
/// `[nal_header, profile_idc, constraint_flags, level_idc, ...]`.
/// The codec string encodes bytes 1–3 as hex.
///
/// Handles SPS both with and without start codes — `CodecParams.sps`
/// contains the raw `NALUnit.data` which includes the Annex B prefix.
///
/// # Panics
///
/// Panics if the stripped SPS is shorter than 4 bytes.
pub fn build_codec_string(sps: &[u8]) -> String {
    let sps = strip_start_code(sps);
    assert!(sps.len() >= 4, "SPS too short to derive codec string ({} bytes)", sps.len());
    format!("avc1.{:02x}{:02x}{:02x}", sps[1], sps[2], sps[3])
}

/// Build an AVCDecoderConfigurationRecord (avcC) from SPS and PPS.
///
/// ISO 14496-15 §5.2.4.1 layout (11 + sps.len + pps.len bytes):
/// ```text
/// [u8  : 1]           configurationVersion
/// [u8×3: profile/compat/level from SPS]
/// [u8  : 0xFF]        lengthSizeMinusOne (→ 4-byte NALU lengths)
/// [u8  : 0xE1]        numSPS=1 + reserved bits
/// [u16 BE: sps.len]   sequenceParameterSetLength
/// [sps bytes]
/// [u8  : 1]           numPPS
/// [u16 BE: pps.len]   pictureParameterSetLength
/// [pps bytes]
/// ```
///
/// Handles SPS/PPS both with and without Annex B start codes —
/// `CodecParams.sps`/`.pps` contain `NALUnit.data` which includes
/// the start code prefix.
///
/// # Panics
///
/// Panics if the stripped SPS is shorter than 4 bytes.
pub fn build_avcc_descriptor(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let sps = strip_start_code(sps);
    let pps = strip_start_code(pps);
    assert!(sps.len() >= 4, "SPS too short for avcC ({} bytes)", sps.len());

    let total = 11 + sps.len() + pps.len();
    let mut buf = Vec::with_capacity(total);

    buf.push(1);        // configurationVersion
    buf.push(sps[1]);   // AVCProfileIndication
    buf.push(sps[2]);   // profile_compatibility
    buf.push(sps[3]);   // AVCLevelIndication
    buf.push(0xFF);     // lengthSizeMinusOne = 3 (→ 4-byte prefixes)
    buf.push(0xE1);     // numSPS = 1 (+ reserved 111 bits)

    buf.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    buf.extend_from_slice(sps);

    buf.push(1);        // numPPS
    buf.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    buf.extend_from_slice(pps);

    buf
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use live_video::{NALUnit, NALUnitType};

    /// Helper: build a single-NAL frame with a 3-byte Annex B start code.
    fn make_frame(is_keyframe: bool, timestamp: u64) -> FrameMessage {
        let nal_type = if is_keyframe { NALUnitType::IDR } else { NALUnitType::NonIDR };
        FrameMessage {
            timestamp_us: timestamp,
            is_keyframe,
            nal_units: vec![NALUnit {
                unit_type: nal_type,
                data: vec![0x00, 0x00, 0x01, 0x65],
            }],
        }
    }

    // ── StreamBuffer tests ──────────────────────────────────────────────

    #[test]
    fn push_and_retrieve() {
        let mut buf = StreamBuffer::new(4);
        buf.push_frame(&make_frame(true, 1000));
        buf.push_frame(&make_frame(false, 2000));

        let frames = buf.get_frames_after(0);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].sequence, 1);
        assert_eq!(frames[1].sequence, 2);
    }

    #[test]
    fn timestamp_preserved_in_buffered_frame() {
        let mut buf = StreamBuffer::new(4);
        buf.push_frame(&make_frame(true, 42_000));

        let frames = buf.get_frames_after(0);
        assert_eq!(frames[0].timestamp_us, 42_000);
    }

    #[test]
    fn keyframe_gating_on_first_request() {
        let mut buf = StreamBuffer::new(4);
        buf.push_frame(&make_frame(false, 1000));
        buf.push_frame(&make_frame(false, 2000));
        buf.push_frame(&make_frame(true, 3000));
        buf.push_frame(&make_frame(false, 4000));

        let frames = buf.get_frames_after(0);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].sequence, 3);
        assert!(frames[0].is_keyframe);
        assert_eq!(frames[1].sequence, 4);
    }

    #[test]
    fn circular_wrap() {
        let mut buf = StreamBuffer::new(3);
        for i in 0..5 {
            buf.push_frame(&make_frame(i == 0 || i == 3, (i + 1) * 1000));
        }

        let frames = buf.get_frames_after(0);
        assert_eq!(frames[0].sequence, 4);
        assert!(frames[0].is_keyframe);
    }

    #[test]
    fn get_after_filters_by_sequence() {
        let mut buf = StreamBuffer::new(4);
        buf.push_frame(&make_frame(true, 1000));
        buf.push_frame(&make_frame(false, 2000));
        buf.push_frame(&make_frame(false, 3000));

        let frames = buf.get_frames_after(2);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].sequence, 3);
    }

    #[test]
    fn reset_clears_everything() {
        let mut buf = StreamBuffer::new(4);
        buf.push_frame(&make_frame(true, 1000));
        buf.set_codec_params(CodecParams {
            sps: vec![0x67], pps: vec![0x68], width: 1920, height: 1080,
        });

        buf.reset();

        assert!(buf.get_codec_params().is_none());
        assert!(buf.get_frames_after(0).is_empty());

        buf.push_frame(&make_frame(true, 5000));
        assert_eq!(buf.get_frames_after(0)[0].sequence, 1);
    }

    #[test]
    fn sequence_monotonicity() {
        let mut buf = StreamBuffer::new(10);
        for i in 0..20 {
            buf.push_frame(&make_frame(i % 5 == 0, i * 1000));
        }

        let frames = buf.get_frames_after(10);
        for pair in frames.windows(2) {
            assert!(pair[1].sequence > pair[0].sequence);
        }
    }

    // ── strip_start_code tests ──────────────────────────────────────────

    #[test]
    fn strip_4byte_start_code() {
        let data = [0x00, 0x00, 0x00, 0x01, 0x65, 0x88];
        assert_eq!(strip_start_code(&data), &[0x65, 0x88]);
    }

    #[test]
    fn strip_3byte_start_code() {
        let data = [0x00, 0x00, 0x01, 0x65, 0x88];
        assert_eq!(strip_start_code(&data), &[0x65, 0x88]);
    }

    #[test]
    fn strip_no_start_code_passthrough() {
        let data = [0x65, 0x88, 0x80];
        assert_eq!(strip_start_code(&data), &[0x65, 0x88, 0x80]);
    }

    #[test]
    fn strip_empty_slice() {
        assert_eq!(strip_start_code(&[]), &[] as &[u8]);
    }

    // ── serialize_avcc_payload tests ────────────────────────────────────

    #[test]
    fn avcc_payload_single_nal_3byte_start_code() {
        let frame = FrameMessage {
            timestamp_us: 1000,
            is_keyframe: false,
            nal_units: vec![NALUnit {
                unit_type: NALUnitType::NonIDR,
                // 3-byte start code + 2 bytes of NAL data
                data: vec![0x00, 0x00, 0x01, 0x41, 0xAA],
            }],
        };

        let payload = serialize_avcc_payload(&frame);
        // Expected: [00 00 00 02] (length=2, big-endian) + [41 AA]
        assert_eq!(payload, vec![0x00, 0x00, 0x00, 0x02, 0x41, 0xAA]);
    }

    #[test]
    fn avcc_payload_multi_nal_mixed_start_codes() {
        // Simulates a keyframe with SPS (4-byte) + PPS (4-byte) + IDR (3-byte)
        let frame = FrameMessage {
            timestamp_us: 5000,
            is_keyframe: true,
            nal_units: vec![
                NALUnit {
                    unit_type: NALUnitType::SPS,
                    data: vec![0x00, 0x00, 0x00, 0x01, 0x67, 0x42],
                },
                NALUnit {
                    unit_type: NALUnitType::PPS,
                    data: vec![0x00, 0x00, 0x00, 0x01, 0x68, 0xCE],
                },
                NALUnit {
                    unit_type: NALUnitType::IDR,
                    data: vec![0x00, 0x00, 0x01, 0x65, 0x88, 0x80],
                },
            ],
        };

        let payload = serialize_avcc_payload(&frame);
        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            // SPS: length=2 (BE) + data
            0x00, 0x00, 0x00, 0x02, 0x67, 0x42,
            // PPS: length=2 (BE) + data
            0x00, 0x00, 0x00, 0x02, 0x68, 0xCE,
            // IDR: length=3 (BE) + data
            0x00, 0x00, 0x00, 0x03, 0x65, 0x88, 0x80,
        ];
        assert_eq!(payload, expected);
    }

    // ── Codec helper tests ──────────────────────────────────────────────

    #[test]
    fn codec_string_baseline_31_no_start_code() {
        // SPS without start code: [nal_header=0x67, profile=0x42, constraints=0x00, level=0x1f]
        let sps = vec![0x67, 0x42, 0x00, 0x1f, 0xE9];
        assert_eq!(build_codec_string(&sps), "avc1.42001f");
    }

    #[test]
    fn codec_string_with_4byte_start_code() {
        // SPS with 4-byte Annex B start code (as stored in CodecParams.sps)
        let sps = vec![0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1f, 0xE9];
        assert_eq!(build_codec_string(&sps), "avc1.42001f");
    }

    #[test]
    fn codec_string_high_40() {
        let sps = vec![0x67, 0x64, 0x00, 0x28];
        assert_eq!(build_codec_string(&sps), "avc1.640028");
    }

    #[test]
    fn avcc_descriptor_strips_start_codes() {
        // SPS/PPS with 4-byte Annex B start codes (real-world CodecParams data)
        let sps = vec![0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0xC0, 0x1E, 0xD9];
        let pps = vec![0x00, 0x00, 0x00, 0x01, 0x68, 0xCE, 0x38, 0x80];

        let desc = build_avcc_descriptor(&sps, &pps);

        // The descriptor should contain stripped data (no start codes)
        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            1,                              // configurationVersion
            0x42, 0xC0, 0x1E,              // profile, compat, level
            0xFF,                           // lengthSizeMinusOne
            0xE1,                           // numSPS=1
            0x00, 0x05,                     // SPS length = 5 (stripped)
            0x67, 0x42, 0xC0, 0x1E, 0xD9, // SPS data (no start code)
            1,                              // numPPS
            0x00, 0x04,                     // PPS length = 4 (stripped)
            0x68, 0xCE, 0x38, 0x80,        // PPS data (no start code)
        ];
        assert_eq!(desc, expected);
    }

    #[test]
    fn avcc_descriptor_without_start_codes() {
        // SPS/PPS already stripped (defensive — should still work)
        let sps = vec![0x67, 0x42, 0xC0, 0x1E, 0xD9];
        let pps = vec![0x68, 0xCE, 0x38, 0x80];

        let desc = build_avcc_descriptor(&sps, &pps);
        assert_eq!(desc.len(), 11 + sps.len() + pps.len());
        // Verify profile/level bytes
        assert_eq!(desc[1], 0x42);
        assert_eq!(desc[2], 0xC0);
        assert_eq!(desc[3], 0x1E);
    }
}
