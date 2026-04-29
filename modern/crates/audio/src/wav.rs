//! WAV audio decoding from raw RIFF WAVE bytes.
//!
//! 3DMM content uses 22050 Hz mono **Microsoft ADPCM** (fmt=2, 4-bit, 512-byte
//! blocks, 1012 samples/block) stored in `CTG_WAVE` chunks.
//! Standard PCM (fmt=1) is also supported for completeness.

use crate::error::AudioError;

// ── WavInfo ──────────────────────────────────────────────────────────────────

/// Metadata about a decoded WAV clip.
#[derive(Debug, Clone)]
pub struct WavInfo {
    /// Audio channels (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Sample rate in Hz (22050 for 3DMM content).
    pub sample_rate: u32,
    /// Audio format code (1 = PCM, 2 = MS-ADPCM).
    pub format: u16,
    /// Total decoded samples (all channels interleaved).
    pub sample_count: usize,
}

// ── RIFF/WAV helpers ─────────────────────────────────────────────────────────

fn read_u16_le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn read_i16_le(b: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([b[off], b[off + 1]])
}
fn read_u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

// ── MS-ADPCM decoder ─────────────────────────────────────────────────────────

// Adaptation table: delta multipliers × 256 for each 4-bit nibble (0-15).
const ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

// Standard MS-ADPCM coefficient pairs (×256).
const COEFF1: [i32; 7] = [256, 512, 0, 192, 240, 460, 392];
const COEFF2: [i32; 7] = [0, -256, 0, 64, 0, -208, -232];

/// Decode one MS-ADPCM nibble (0-15) in-place.
/// Returns the new PCM sample as i16.
#[inline]
fn decode_nibble(nibble: i32, pred_idx: usize, delta: &mut i32, s1: &mut i32, s2: &mut i32) -> i16 {
    // Sign-extend nibble from 4-bit two's complement
    let n = if nibble >= 8 { nibble - 16 } else { nibble };

    let pred = (COEFF1[pred_idx] * *s1 + COEFF2[pred_idx] * *s2) / 256 + n * *delta;
    let pred = pred.clamp(i16::MIN as i32, i16::MAX as i32);

    *s2 = *s1;
    *s1 = pred;
    *delta = (*delta * ADAPT[nibble as usize] / 256).max(16);

    pred as i16
}

/// Decode one MS-ADPCM block (mono or stereo).
/// Returns PCM i16 samples (all channels interleaved).
fn decode_ms_adpcm_block(block: &[u8], channels: usize) -> Vec<i16> {
    // Per-channel header: [pred_idx:1][delta:2][samp1:2][samp2:2] = 7 bytes
    // All pred_idx bytes come first, then all delta, then all samp1, then all samp2
    // (that's the standard MS-ADPCM layout for multi-channel)
    let header = 7 * channels;
    if block.len() < header {
        return Vec::new();
    }

    let mut pred_idx = vec![0usize; channels];
    let mut delta = vec![0i32; channels];
    let mut s1 = vec![0i32; channels];
    let mut s2 = vec![0i32; channels];

    // Read headers: pred_idx[ch0], pred_idx[ch1], ... (one byte each)
    for ch in 0..channels {
        pred_idx[ch] = block[ch].min(6) as usize;
    }
    let mut pos = channels;
    for ch in 0..channels {
        delta[ch] = read_i16_le(block, pos) as i32;
        pos += 2;
    }
    for ch in 0..channels {
        s1[ch] = read_i16_le(block, pos) as i32;
        pos += 2;
    }
    for ch in 0..channels {
        s2[ch] = read_i16_le(block, pos) as i32;
        pos += 2;
    }

    // The two header samples output in order: s2[ch0..], s1[ch0..]
    let mut out = Vec::new();
    for ch in 0..channels {
        out.push(s2[ch] as i16);
    }
    for ch in 0..channels {
        out.push(s1[ch] as i16);
    }

    // Decode nibble pairs; nibble pairs alternate channels
    let mut ch = 0usize;
    while pos < block.len() {
        let byte = block[pos] as i32;
        pos += 1;

        // High nibble
        let hi = (byte >> 4) & 0xF;
        out.push(decode_nibble(
            hi,
            pred_idx[ch],
            &mut delta[ch],
            &mut s1[ch],
            &mut s2[ch],
        ));
        ch = (ch + 1) % channels;

        // Low nibble
        let lo = byte & 0xF;
        out.push(decode_nibble(
            lo,
            pred_idx[ch],
            &mut delta[ch],
            &mut s1[ch],
            &mut s2[ch],
        ));
        ch = (ch + 1) % channels;
    }

    out
}

// ── PCM decoder ───────────────────────────────────────────────────────────────

fn decode_pcm(data_chunk: &[u8], bits_per_sample: u16) -> Result<Vec<i16>, AudioError> {
    match bits_per_sample {
        8 => {
            // WAV 8-bit PCM is unsigned (0..255); convert to i16 centered on 0.
            Ok(data_chunk
                .iter()
                .map(|&b| ((b as i16) - 128) * 256)
                .collect())
        }
        16 => {
            if data_chunk.len() % 2 != 0 {
                return Err(AudioError::WavDecode(
                    "odd data chunk length for 16-bit PCM".into(),
                ));
            }
            Ok(data_chunk
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect())
        }
        bps => Err(AudioError::WavDecode(format!(
            "unsupported PCM bits_per_sample={bps}"
        ))),
    }
}

// ── decode_wav (public) ───────────────────────────────────────────────────────

/// Decode raw RIFF WAVE bytes into i16 PCM samples.
///
/// Supports:
/// - fmt=1 (PCM, 8-bit and 16-bit)
/// - fmt=2 (Microsoft ADPCM, 4-bit — the format used by 3DMM content)
///
/// Returns `(WavInfo, samples)` where `samples` contains all channels interleaved.
pub fn decode_wav(data: &[u8]) -> Result<(WavInfo, Vec<i16>), AudioError> {
    // ── Validate RIFF/WAVE header ──
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(AudioError::WavDecode("not a RIFF WAVE file".into()));
    }

    // ── Walk RIFF chunks ──
    let mut format: u16 = 0;
    let mut channels: u16 = 0;
    let mut sample_rate: u32 = 0;
    let mut bits_per_sample: u16 = 0;
    let mut block_align: u16 = 0;
    let mut samples_per_block: u16 = 0;
    let mut data_chunk: Option<&[u8]> = None;

    let mut off = 12usize;
    while off + 8 <= data.len() {
        let tag = &data[off..off + 4];
        let chunk_size = read_u32_le(data, off + 4) as usize;
        let body_start = off + 8;
        let body_end = body_start + chunk_size;
        if body_end > data.len() {
            break;
        }
        let body = &data[body_start..body_end];

        if tag == b"fmt " {
            if body.len() < 16 {
                return Err(AudioError::WavDecode("fmt chunk too small".into()));
            }
            format = read_u16_le(body, 0);
            channels = read_u16_le(body, 2);
            sample_rate = read_u32_le(body, 4);
            block_align = read_u16_le(body, 12);
            bits_per_sample = read_u16_le(body, 14);
            // Extended fmt (cbSize > 0)
            if format == 2 && body.len() >= 20 {
                samples_per_block = read_u16_le(body, 18);
            }
        } else if tag == b"data" {
            data_chunk = Some(body);
        }

        // Advance past chunk (word-aligned)
        off = body_end + (chunk_size % 2);
    }

    let data_bytes =
        data_chunk.ok_or_else(|| AudioError::WavDecode("no data chunk found".into()))?;

    if channels == 0 {
        return Err(AudioError::WavDecode(
            "fmt chunk missing or channels=0".into(),
        ));
    }

    let samples = match format {
        1 => decode_pcm(data_bytes, bits_per_sample)?,
        2 => {
            // MS-ADPCM: decode block by block
            if block_align == 0 {
                return Err(AudioError::WavDecode("ADPCM block_align=0".into()));
            }
            let _ = samples_per_block; // computed from block, not needed explicitly
            let mut out = Vec::new();
            let block_size = block_align as usize;
            let ch = channels as usize;
            let mut pos = 0usize;
            while pos + block_size <= data_bytes.len() {
                let block = &data_bytes[pos..pos + block_size];
                out.extend(decode_ms_adpcm_block(block, ch));
                pos += block_size;
            }
            // Decode partial last block if present
            if pos < data_bytes.len() {
                out.extend(decode_ms_adpcm_block(&data_bytes[pos..], ch));
            }
            out
        }
        fmt => return Err(AudioError::WavDecode(format!("unsupported fmt={fmt}"))),
    };

    let sample_count = samples.len();
    Ok((
        WavInfo {
            channels,
            sample_rate,
            format,
            sample_count,
        },
        samples,
    ))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_riff_wav_pcm8(data: &[u8]) -> Vec<u8> {
        let data_size = data.len() as u32;
        let fmt_size: u32 = 16;
        let riff_size = 4 + (8 + fmt_size) + (8 + data_size);
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&fmt_size.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&22050u32.to_le_bytes());
        buf.extend_from_slice(&22050u32.to_le_bytes()); // byte_rate
        buf.extend_from_slice(&1u16.to_le_bytes()); // block_align
        buf.extend_from_slice(&8u16.to_le_bytes()); // bits
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(data);
        buf
    }

    #[test]
    fn decode_pcm8_mono() {
        let raw = make_riff_wav_pcm8(&[128u8, 200, 50, 100]);
        let (info, samples) = decode_wav(&raw).unwrap();
        assert_eq!(info.channels, 1);
        assert_eq!(info.sample_rate, 22050);
        assert_eq!(info.format, 1);
        assert_eq!(samples.len(), 4);
        // 128 → 0 (center), 200 → positive, 50 → negative
        assert_eq!(samples[0], 0);
        assert!(samples[1] > 0);
        assert!(samples[2] < 0);
    }

    #[test]
    fn reject_non_riff() {
        assert!(decode_wav(b"MIDI....").is_err());
        assert!(decode_wav(&[]).is_err());
    }

    #[test]
    fn decode_ms_adpcm_nibble_center() {
        // nibble 0 with delta=16, s1=0, s2=0, coeff[0]=(256,0)
        // pred = (256*0 + 0*0)/256 + 0*16 = 0
        let mut delta = 16i32;
        let mut s1 = 0i32;
        let mut s2 = 0i32;
        let out = decode_nibble(0, 0, &mut delta, &mut s1, &mut s2);
        assert_eq!(out, 0);
        // delta adaptation: 16 * 230 / 256 = 14 → clamped to 16
        assert_eq!(delta, 16);
    }
}
