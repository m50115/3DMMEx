//! Movie sound chunk (MSND / MSNDF).
//!
//! MSNDF on-disk layout (16 bytes, always LE):
//!   [0..2]  bo:i16        — byte order (1 = LE)
//!   [2..4]  osk:i16       — OS kind (0x7769 = Windows)
//!   [4..8]  sty:i32       — sound type (SoundType enum)
//!   [8..12] vlm_default:i32 — default volume (0x7FFF = max)
//!   [12]    f_invalid:u8  — 1 if this sound is invalid/placeholder
//!   [13..16] padding

use crate::error::{EngineError, EngineResult};

// ── SoundType ────────────────────────────────────────────────────────────────

/// Sound type (sty field in MSNDF).
///
/// Determines whether the audio data is WAV (Sfx/Speech) or MIDI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundType {
    /// No sound (silent placeholder, e.g. "sound stopper").
    Nil = 0,
    /// Legacy/unused slot.
    Unused = 1,
    /// Sound effect (WAV, stored as WAVE child of MSND).
    Sfx = 2,
    /// Speech/voice (WAV, stored as WAVE child of MSND).
    Speech = 3,
    /// Background music (MIDI, stored as MIDS child of MSND).
    Midi = 4,
}

impl SoundType {
    pub fn is_wav(self) -> bool {
        matches!(self, SoundType::Sfx | SoundType::Speech)
    }

    pub fn is_midi(self) -> bool {
        self == SoundType::Midi
    }
}

impl TryFrom<i32> for SoundType {
    type Error = EngineError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(SoundType::Nil),
            1 => Ok(SoundType::Unused),
            2 => Ok(SoundType::Sfx),
            3 => Ok(SoundType::Speech),
            4 => Ok(SoundType::Midi),
            _ => Err(EngineError::OutOfRange {
                what: "sty",
                value: v as i64,
            }),
        }
    }
}

// ── MovieSound ───────────────────────────────────────────────────────────────

/// Parsed MSND chunk (MSNDF 16 bytes on-disk).
///
/// Audio data is in a child chunk:
/// - `SoundType::Sfx` / `Speech` → child `CTG_WAVE` (raw RIFF WAV)
/// - `SoundType::Midi` → child `CTG_MIDS` (Standard MIDI File)
/// - `SoundType::Nil` / `Unused` → no child (silent)
#[derive(Debug, Clone)]
pub struct MovieSound {
    /// Sound type — determines which child chunk format to expect.
    pub sty: SoundType,
    /// Default playback volume (0x7FFF = maximum).
    pub vlm_default: i32,
    /// True if this slot is an invalid/placeholder sound.
    pub invalid: bool,
}

impl MovieSound {
    /// Size of the on-disk MSNDF struct in bytes.
    pub const SIZE: usize = 16;

    /// Parse an MSNDF from the first `SIZE` bytes of `data`.
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < Self::SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "MSNDF",
                need: Self::SIZE,
                got: data.len(),
            });
        }

        let bo = i16::from_le_bytes([data[0], data[1]]);
        if bo != 1 {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }

        let sty_raw = i32::from_le_bytes(data[4..8].try_into().unwrap());
        let vlm_default = i32::from_le_bytes(data[8..12].try_into().unwrap());
        let invalid = data[12] != 0;

        Ok(Self {
            sty: SoundType::try_from(sty_raw)?,
            vlm_default,
            invalid,
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msndf(sty: i32, vlm: i32, invalid: bool) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..2].copy_from_slice(&1i16.to_le_bytes()); // bo = 1
        b[2..4].copy_from_slice(&0x7769i16.to_le_bytes()); // osk = Windows
        b[4..8].copy_from_slice(&sty.to_le_bytes());
        b[8..12].copy_from_slice(&vlm.to_le_bytes());
        b[12] = if invalid { 1 } else { 0 };
        b
    }

    #[test]
    fn sfx_sound() {
        let data = make_msndf(2, 0x7FFF, false);
        let msnd = MovieSound::from_bytes(&data).unwrap();
        assert_eq!(msnd.sty, SoundType::Sfx);
        assert!(msnd.sty.is_wav());
        assert!(!msnd.sty.is_midi());
        assert_eq!(msnd.vlm_default, 0x7FFF);
        assert!(!msnd.invalid);
    }

    #[test]
    fn midi_sound() {
        let data = make_msndf(4, 0x4000, false);
        let msnd = MovieSound::from_bytes(&data).unwrap();
        assert_eq!(msnd.sty, SoundType::Midi);
        assert!(msnd.sty.is_midi());
        assert!(!msnd.sty.is_wav());
    }

    #[test]
    fn nil_sound() {
        let data = make_msndf(0, 0, false);
        let msnd = MovieSound::from_bytes(&data).unwrap();
        assert_eq!(msnd.sty, SoundType::Nil);
        assert!(!msnd.sty.is_wav());
        assert!(!msnd.sty.is_midi());
    }

    #[test]
    fn invalid_flag() {
        let data = make_msndf(2, 0, true);
        let msnd = MovieSound::from_bytes(&data).unwrap();
        assert!(msnd.invalid);
    }

    #[test]
    fn wrong_byte_order() {
        let mut data = make_msndf(2, 0, false);
        data[0] = 0xFF;
        data[1] = 0xFF;
        assert!(MovieSound::from_bytes(&data).is_err());
    }

    #[test]
    fn too_short() {
        assert!(MovieSound::from_bytes(&[0u8; 8]).is_err());
    }

    #[test]
    fn speech_sound() {
        let data = make_msndf(3, 0x3000, false);
        let msnd = MovieSound::from_bytes(&data).unwrap();
        assert_eq!(msnd.sty, SoundType::Speech);
        assert!(msnd.sty.is_wav());
    }
}
