//! TDF — Three-D Font parser.
//!
//! On-disk layout of the TDFF header (12 bytes):
//!   [bo:i16][osk:i16][cch:u32][dyrMax:i32(BRS)]
//! Followed by:
//!   rgdxr: [i32 × cch]   per-character x spacing (BRS)
//!   rgdyr: [i32 × cch]   per-character y advance  (BRS)
//!
//! BMDL children of the TDF chunk are keyed by chid == ASCII codepoint.
//!
//! Reference: src/engine/tdf.cpp (original C++ source).

use crate::error::EngineError;
use crate::fixedpoint::FixedScalar;

/// Byte-order marker for the current platform (little-endian = 1).
const KBO_CUR: i16 = 0x0001;

/// Maximum character count sanity limit.
const CCH_MAX: u32 = 512;

/// Parsed Three-D Font data from a TDFF chunk.
#[derive(Debug, Clone)]
pub struct BrTdf {
    /// Number of characters in the font.
    pub cch: u32,
    /// Maximum character height (BRS → f32).
    pub dyr_max: f32,
    /// Per-character x spacing, len == cch (BRS → f32).
    pub dxr: Vec<f32>,
    /// Per-character y advance, len == cch (BRS → f32).
    pub dyr: Vec<f32>,
}

impl BrTdf {
    /// Parse a TDFF chunk body.
    ///
    /// `bytes` is the raw decompressed chunk data (not including any chunky
    /// file framing — just the payload bytes).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        if bytes.len() < 12 {
            return Err(EngineError::UnexpectedEof {
                what: "TDFF header",
                need: 12,
                got: bytes.len(),
            });
        }

        let bo = i16::from_le_bytes(bytes[0..2].try_into().unwrap());
        if bo != KBO_CUR {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }

        // osk at [2..4] — ignored
        let cch = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let dyr_max_raw = i32::from_le_bytes(bytes[8..12].try_into().unwrap());

        if cch == 0 || cch > CCH_MAX {
            return Err(EngineError::OutOfRange {
                what: "TDFF cch",
                value: cch as i64,
            });
        }

        let required = 12 + (cch as usize) * 8;
        if bytes.len() < required {
            return Err(EngineError::UnexpectedEof {
                what: "TDFF arrays",
                need: required,
                got: bytes.len(),
            });
        }

        let dyr_max = FixedScalar(dyr_max_raw).to_f64() as f32;

        let mut dxr = Vec::with_capacity(cch as usize);
        let mut dyr = Vec::with_capacity(cch as usize);

        let dxr_base = 12usize;
        let dyr_base = 12 + (cch as usize) * 4;

        for i in 0..cch as usize {
            let xo = dxr_base + i * 4;
            let yo = dyr_base + i * 4;
            let xv = i32::from_le_bytes(bytes[xo..xo + 4].try_into().unwrap());
            let yv = i32::from_le_bytes(bytes[yo..yo + 4].try_into().unwrap());
            dxr.push(FixedScalar(xv).to_f64() as f32);
            dyr.push(FixedScalar(yv).to_f64() as f32);
        }

        Ok(Self {
            cch,
            dyr_max,
            dxr,
            dyr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tdff(cch: u32, dyr_max: i32, dxr: &[i32], dyr: &[i32]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1i16.to_le_bytes()); // bo = kboCur
        b.extend_from_slice(&0i16.to_le_bytes()); // osk
        b.extend_from_slice(&cch.to_le_bytes());
        b.extend_from_slice(&dyr_max.to_le_bytes());
        for &v in dxr {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for &v in dyr {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn parse_single_char_font() {
        // 1 character, dyrMax=1.0 (65536), dxr=[0.5], dyr=[1.0]
        let bytes = make_tdff(1, 65536, &[32768], &[65536]);
        let tdf = BrTdf::from_bytes(&bytes).unwrap();
        assert_eq!(tdf.cch, 1);
        assert!((tdf.dyr_max - 1.0).abs() < 1e-5, "dyr_max={}", tdf.dyr_max);
        assert_eq!(tdf.dxr.len(), 1);
        assert_eq!(tdf.dyr.len(), 1);
        assert!((tdf.dxr[0] - 0.5).abs() < 1e-4, "dxr[0]={}", tdf.dxr[0]);
        assert!((tdf.dyr[0] - 1.0).abs() < 1e-5, "dyr[0]={}", tdf.dyr[0]);
    }

    #[test]
    fn parse_multi_char_font() {
        let cch = 10u32;
        let dxr: Vec<i32> = (0..cch as i32).map(|i| (i + 1) * 6554).collect();
        let dyr: Vec<i32> = vec![65536; cch as usize];
        let bytes = make_tdff(cch, 65536 * 2, &dxr, &dyr);
        let tdf = BrTdf::from_bytes(&bytes).unwrap();
        assert_eq!(tdf.cch, cch);
        assert_eq!(tdf.dxr.len(), cch as usize);
        assert_eq!(tdf.dyr.len(), cch as usize);
        assert!((tdf.dyr_max - 2.0).abs() < 1e-4);
    }

    #[test]
    fn reject_wrong_byte_order() {
        let mut bytes = make_tdff(1, 65536, &[0], &[65536]);
        bytes[0] = 0x02; // bo = 2, not kboCur
        bytes[1] = 0x00;
        assert!(BrTdf::from_bytes(&bytes).is_err());
    }

    #[test]
    fn reject_zero_cch() {
        let mut bytes = make_tdff(1, 65536, &[0], &[65536]);
        // overwrite cch = 0
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(BrTdf::from_bytes(&bytes).is_err());
    }

    #[test]
    fn reject_too_short() {
        // Header only, no arrays
        let mut bytes = make_tdff(5, 65536, &[0; 5], &[65536; 5]);
        bytes.truncate(12); // cut off arrays
        assert!(BrTdf::from_bytes(&bytes).is_err());
    }
}
