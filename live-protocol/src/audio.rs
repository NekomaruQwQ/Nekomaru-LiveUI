//! Audio-specific payload serialization and deserialization.
//!
//! These functions serialize/deserialize the *payload* portion of audio
//! messages.  The 8-byte frame header is handled by the parent module.

use std::io;

// ── AudioConfig Payload ─────────────────────────────────────────────────────

/// Audio format parameters sent once at capture start.
///
/// Payload layout (8 bytes, 4-byte aligned):
/// ```text
/// [u32 LE: sample_rate][u8: channels][u8: bits_per_sample][u16: reserved=0]
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioConfig {
    /// Samples per second (e.g. 48000).
    pub sample_rate: u32,
    /// Number of audio channels (e.g. 2 for stereo).
    pub channels: u8,
    /// Bits per sample in the PCM output (e.g. 16 for s16le).
    pub bits_per_sample: u8,
}

/// Serialize `AudioConfig` into the payload byte format.
pub fn write_audio_config_payload(config: &AudioConfig) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    buf.extend_from_slice(&config.sample_rate.to_le_bytes());
    buf.push(config.channels);
    buf.push(config.bits_per_sample);
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
    buf
}

/// Deserialize `AudioConfig` from a payload byte slice.
#[expect(clippy::missing_asserts_for_indexing, reason = "length check above guards all accesses")]
pub fn read_audio_config_payload(data: &[u8]) -> io::Result<AudioConfig> {
    if data.len() < 6 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated AudioConfig payload"));
    }
    let sample_rate = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let channels = data[4];
    let bits_per_sample = data[5];
    Ok(AudioConfig { sample_rate, channels, bits_per_sample })
}

// ── AudioChunk Payload ──────────────────────────────────────────────────────

/// Serialize an audio chunk payload.
///
/// Payload layout:
/// ```text
/// [u64 LE: timestamp_us][interleaved s16le PCM bytes]
/// ```
pub fn write_audio_chunk_payload(timestamp_us: u64, pcm_data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + pcm_data.len());
    buf.extend_from_slice(&timestamp_us.to_le_bytes());
    buf.extend_from_slice(pcm_data);
    buf
}

/// Parse an audio chunk payload.  Returns `(timestamp_us, pcm_data)`.
pub fn read_audio_chunk_payload(data: &[u8]) -> io::Result<(u64, &[u8])> {
    if data.len() < 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated AudioChunk payload"));
    }
    let bytes: [u8; 8] = data[..8].try_into()
        .map_err(|_err| io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated AudioChunk payload"))?;
    let timestamp_us = u64::from_le_bytes(bytes);
    Ok((timestamp_us, &data[8..]))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_config_roundtrip() {
        let config = AudioConfig {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
        };

        let payload = write_audio_config_payload(&config);
        assert_eq!(payload.len(), 8, "AudioConfig payload should be 8 bytes (4-byte aligned)");

        let decoded = read_audio_config_payload(&payload).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn audio_config_non_standard_rate() {
        let config = AudioConfig {
            sample_rate: 44100,
            channels: 1,
            bits_per_sample: 16,
        };

        let payload = write_audio_config_payload(&config);
        let decoded = read_audio_config_payload(&payload).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn audio_chunk_roundtrip() {
        // 4 stereo s16le samples (16 bytes)
        let pcm: Vec<u8> = vec![
            0x00, 0x10, 0x00, 0x20,
            0xFF, 0x7F, 0x01, 0x80,
            0x00, 0x00, 0x00, 0x00,
            0xAB, 0xCD, 0xEF, 0x01,
        ];
        let payload = write_audio_chunk_payload(1_000_000, &pcm);

        let (ts, data) = read_audio_chunk_payload(&payload).unwrap();
        assert_eq!(ts, 1_000_000);
        assert_eq!(data, pcm);
    }

    #[test]
    fn audio_chunk_empty_pcm() {
        let payload = write_audio_chunk_payload(0, &[]);
        let (ts, data) = read_audio_chunk_payload(&payload).unwrap();
        assert_eq!(ts, 0);
        assert!(data.is_empty());
    }

    #[test]
    fn audio_config_payload_too_short() {
        assert!(read_audio_config_payload(&[0; 5]).is_err());
    }

    #[test]
    fn audio_chunk_payload_too_short() {
        assert!(read_audio_chunk_payload(&[0; 7]).is_err());
    }
}
