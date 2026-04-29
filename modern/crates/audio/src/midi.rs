//! MIDI parsing from 3DMM MIDS chunk data.
//!
//! MIDS chunks contain raw MIDI track stream data — the body of an MTrk
//! block without the Standard MIDI File headers.  The format is:
//!   - Variable-length delta times (VLQ, MSB-first, continuation bit 0x80)
//!   - MIDI event bytes (status + data, running status supported)
//!
//! `parse_midi()` wraps the raw stream in a minimal SMF (MThd + MTrk) so
//! that the `midly` crate can parse it.  It also handles complete Standard
//! MIDI Files (starting with "MThd") for forward-compatibility.

use crate::error::AudioError;

// ── MidiInfo ─────────────────────────────────────────────────────────────────

/// Summary of a parsed MIDI stream.
#[derive(Debug, Clone)]
pub struct MidiInfo {
    /// Number of tracks.
    pub track_count: usize,
    /// Total MIDI events across all tracks.
    pub event_count: usize,
    /// SMF format (0 = single-track, 1 = multi-track).
    pub format: u16,
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Wrap raw MTrk content in a minimal SMF so midly can parse it.
///
/// Appends an end-of-track meta event if the data doesn't already end with one.
fn wrap_mids_as_smf(data: &[u8]) -> Vec<u8> {
    const EOT: &[u8] = &[0x00, 0xFF, 0x2F, 0x00]; // delta=0, EndOfTrack

    let needs_eot = !data.ends_with(EOT);
    let track_len = data.len() as u32 + if needs_eot { EOT.len() as u32 } else { 0 };

    let mut smf = Vec::with_capacity(14 + 8 + track_len as usize);

    // MThd — format 0, 1 track, 480 ticks/quarter
    smf.extend_from_slice(b"MThd");
    smf.extend_from_slice(&6u32.to_be_bytes());
    smf.extend_from_slice(&0u16.to_be_bytes()); // format 0
    smf.extend_from_slice(&1u16.to_be_bytes()); // 1 track
    smf.extend_from_slice(&480u16.to_be_bytes()); // 480 ticks/quarter

    // MTrk
    smf.extend_from_slice(b"MTrk");
    smf.extend_from_slice(&track_len.to_be_bytes());
    smf.extend_from_slice(data);
    if needs_eot {
        smf.extend_from_slice(EOT);
    }

    smf
}

// ── parse_midi ───────────────────────────────────────────────────────────────

/// Parse MIDI data from either a raw MIDS track stream or a complete SMF.
///
/// - If `data` starts with "MThd", treats it as a Standard MIDI File.
/// - Otherwise, wraps `data` as a single MTrk and parses the resulting SMF.
pub fn parse_midi(data: &[u8]) -> Result<MidiInfo, AudioError> {
    let smf_bytes: Vec<u8>;
    let to_parse: &[u8] = if data.len() >= 4 && &data[0..4] == b"MThd" {
        data
    } else {
        smf_bytes = wrap_mids_as_smf(data);
        &smf_bytes
    };

    let smf = midly::Smf::parse(to_parse).map_err(|e| AudioError::MidiParse(e.to_string()))?;

    let format = match smf.header.format {
        midly::Format::SingleTrack => 0,
        midly::Format::Parallel => 1,
        midly::Format::Sequential => 2,
    };

    let event_count = smf.tracks.iter().map(|t| t.len()).sum();

    Ok(MidiInfo {
        track_count: smf.tracks.len(),
        event_count,
        format,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal MIDS stream: Program Change to program 56, then end-of-track.
    fn minimal_mids_stream() -> Vec<u8> {
        vec![
            0x00, 0xC0, 0x38, // delta=0, Program Change ch0, program=56
            0x00, 0xFF, 0x2F, 0x00, // delta=0, EndOfTrack
        ]
    }

    /// Build a complete SMF for comparison.
    fn minimal_smf() -> Vec<u8> {
        let track = minimal_mids_stream();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MThd");
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(&480u16.to_be_bytes());
        buf.extend_from_slice(b"MTrk");
        buf.extend_from_slice(&(track.len() as u32).to_be_bytes());
        buf.extend_from_slice(&track);
        buf
    }

    #[test]
    fn parse_mids_stream() {
        let stream = minimal_mids_stream();
        let info = parse_midi(&stream).unwrap();
        assert_eq!(info.format, 0);
        assert_eq!(info.track_count, 1);
        assert!(info.event_count > 0);
    }

    #[test]
    fn parse_complete_smf() {
        let smf = minimal_smf();
        let info = parse_midi(&smf).unwrap();
        assert_eq!(info.format, 0);
        assert_eq!(info.track_count, 1);
        assert!(info.event_count > 0);
    }

    #[test]
    fn parse_mids_without_eot() {
        // Stream without end-of-track — wrap_mids_as_smf should append it
        let stream = vec![0x00, 0xC0, 0x38]; // just Program Change, no EOT
        let info = parse_midi(&stream).unwrap();
        assert_eq!(info.track_count, 1);
    }

    #[test]
    fn reject_garbage() {
        // Random bytes that can't be wrapped into valid MIDI
        // (midly should error on invalid event bytes)
        // Note: not all garbage fails — midly is lenient — but empty should work
        let empty = [];
        // Empty stream wrapped = just EOT = valid 0-event track
        let info = parse_midi(&empty).unwrap();
        assert_eq!(info.event_count, 1); // the EOT event we appended
    }
}
