//! TDT — Three-D Text parser.
//!
//! On-disk layout of the TDTF chunk body (24 bytes):
//!   [bo:i16][osk:i16][tdts:i32][tagTdf:TAGF(16)]
//!
//! `tagTdf` is a TAGF (16 bytes) pointing to a TDF (Three-D Font) chunk in
//! tdfs.3cn.  `tagTdf.cno` is the font chunk cno; `tagTdf.ctg == CTG_TDF`.
//!
//! The text string to render (_stn) is stored as the TMPL chunk's name
//! (already exposed as `ChunkEntry.name: Option<String>` in chunky-format).
//!
//! Reference: src/engine/tdt.cpp, inc/tdt.h (original C++ source).

use crate::error::EngineError;
use crate::tag::{parse_tagf, TagOnFile};

/// Byte-order marker for the current platform (little-endian = 1).
const KBO_CUR: i16 = 0x0001;

/// Three-D Text shape variant.
///
/// Unknown values read from disk are silently mapped to `Normal` so that fan
/// movies with unusual sentinel values still render instead of hard-failing.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tdts {
    Normal = 0,
    ArchPositive = 1,
    CircleY = 2,
    LargeMiddle = 3,
    ArchNegative = 4,
    ArchZ = 5,
    CircleZ = 6,
    Vertical = 7,
    GrowRight = 8,
    GrowLeft = 9,
}

impl Tdts {
    /// Convert a raw i32 disk value to a `Tdts`.  Unknown values → `Normal`.
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Normal,
            1 => Self::ArchPositive,
            2 => Self::CircleY,
            3 => Self::LargeMiddle,
            4 => Self::ArchNegative,
            5 => Self::ArchZ,
            6 => Self::CircleZ,
            7 => Self::Vertical,
            8 => Self::GrowRight,
            9 => Self::GrowLeft,
            other => {
                eprintln!("tdt: unknown tdts value {other}, treating as Normal");
                Self::Normal
            }
        }
    }
}

/// Parsed Three-D Text data from a TDTF chunk.
#[derive(Debug, Clone)]
pub struct BrTdt {
    /// Shape variant controlling character layout.
    pub tdts: Tdts,
    /// Reference to the TDF (Three-D Font) chunk that supplies glyph BMDLs.
    /// `tag_tdf.cno` is the TDF chunk cno in tdfs.3cn.
    pub tag_tdf: TagOnFile,
}

impl BrTdt {
    /// Parse a TDTF chunk body.  Expects exactly 24 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        if bytes.len() < 24 {
            return Err(EngineError::UnexpectedEof {
                what: "TDTF",
                need: 24,
                got: bytes.len(),
            });
        }

        let bo = i16::from_le_bytes(bytes[0..2].try_into().unwrap());
        if bo != KBO_CUR {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }

        // osk at [2..4] — ignored
        let tdts_raw = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let tdts = Tdts::from_i32(tdts_raw);

        // tagTdf: TAGF at bytes[8..24]
        let tag_tdf = parse_tagf(&bytes[8..24])?;

        Ok(Self { tdts, tag_tdf })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::CTG_TDF;

    /// Build a minimal valid TDTF byte buffer.
    fn make_tdtf(tdts: i32, sid: i32, ctg: u32, cno: u32) -> Vec<u8> {
        let mut b = Vec::with_capacity(24);
        b.extend_from_slice(&1i16.to_le_bytes()); // bo = kboCur
        b.extend_from_slice(&0i16.to_le_bytes()); // osk
        b.extend_from_slice(&tdts.to_le_bytes());
        // TAGF: sid(4) + _pcrf(4) + ctg(4) + cno(4)
        b.extend_from_slice(&sid.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes()); // _pcrf = 0
        b.extend_from_slice(&ctg.to_le_bytes());
        b.extend_from_slice(&cno.to_le_bytes());
        b
    }

    #[test]
    fn parse_normal_tdt() {
        let bytes = make_tdtf(0, 0, CTG_TDF, 42);
        let tdt = BrTdt::from_bytes(&bytes).unwrap();
        assert_eq!(tdt.tdts, Tdts::Normal);
        assert_eq!(tdt.tag_tdf.ctg, CTG_TDF);
        assert_eq!(tdt.tag_tdf.cno, 42);
    }

    #[test]
    fn parse_all_known_tdts() {
        let cases = [
            (0, Tdts::Normal),
            (1, Tdts::ArchPositive),
            (2, Tdts::CircleY),
            (3, Tdts::LargeMiddle),
            (4, Tdts::ArchNegative),
            (5, Tdts::ArchZ),
            (6, Tdts::CircleZ),
            (7, Tdts::Vertical),
            (8, Tdts::GrowRight),
            (9, Tdts::GrowLeft),
        ];
        for (raw, expected) in cases {
            let bytes = make_tdtf(raw, 0, CTG_TDF, 1);
            let tdt = BrTdt::from_bytes(&bytes).unwrap();
            assert_eq!(tdt.tdts, expected, "tdts raw={raw}");
        }
    }

    #[test]
    fn unknown_tdts_maps_to_normal() {
        let bytes = make_tdtf(99, 0, CTG_TDF, 1);
        let tdt = BrTdt::from_bytes(&bytes).unwrap();
        assert_eq!(tdt.tdts, Tdts::Normal);
    }

    #[test]
    fn reject_wrong_byte_order() {
        let mut bytes = make_tdtf(0, 0, CTG_TDF, 1);
        bytes[0] = 0x02;
        bytes[1] = 0x00; // bo = 2
        assert!(BrTdt::from_bytes(&bytes).is_err());
    }

    #[test]
    fn reject_too_short() {
        let bytes = make_tdtf(0, 0, CTG_TDF, 1);
        assert!(BrTdt::from_bytes(&bytes[..20]).is_err());
    }

    #[test]
    fn tagf_sid_and_cno_preserved() {
        let bytes = make_tdtf(1, -1, CTG_TDF, 0xDEAD_BEEF);
        let tdt = BrTdt::from_bytes(&bytes).unwrap();
        assert_eq!(tdt.tag_tdf.sid, -1);
        assert_eq!(tdt.tag_tdf.cno, 0xDEAD_BEEF);
    }
}
