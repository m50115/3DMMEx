//! BRender material (MTRL chunk) domain types and parsing.
//!
//! On-disk layout (MTRLF, 20 bytes):
//!   [bo:i16][osk:i16][colour:u32][ka:u16][kd:u16][ks:u16]
//!   [index_base:u8][index_range:u8][power:i32]
//!
//! Type notes:
//!   colour      = br_colour (u32): 0x00RRGGBB
//!   ka/kd/ks    = br_ufraction (u16): 0x0000=0.0, 0xFFFF≈1.0
//!   power       = BRS (i32): 16.16 signed fixed-point (same as FixedScalar)

use crate::error::{EngineError, EngineResult};
use crate::fixedpoint::FixedScalar;

// ── Constants ───────────────────────────────────────────────────────────────

/// Expected byte-order marker for little-endian MTRLF.
const BO_LITTLE_ENDIAN: i16 = 0x0001;

/// On-disk size of MTRLF.
pub const MTRLF_SIZE: usize = 20;

// ── BrMaterial — 20 bytes on disk ─────────────────────────────────────────

/// BRender material as stored on disk (MTRLF). 20 bytes.
///
/// Conversion to GPU types:
/// - `colour`  → R=(c>>16)&0xFF, G=(c>>8)&0xFF, B=c&0xFF; each /255.0
/// - `ka/kd/ks` → value as f32 / 65535.0   (br_ufraction u16)
/// - `power`   → BRS to f32: value / 65536.0
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrMaterial {
    /// RGB color packed as 0x00RRGGBB.
    pub colour: u32,
    /// Ambient coefficient (br_ufraction u16: 0=0.0, 65535≈1.0).
    pub ka: u16,
    /// Diffuse coefficient.
    pub kd: u16,
    /// Specular coefficient.
    pub ks: u16,
    /// Palette index base (0 when not using palette rendering).
    pub index_base: u8,
    /// Palette index range (0 when not using palette rendering).
    pub index_range: u8,
    /// Specular exponent (BRS 16.16 fixed-point; e.g. 0x00320000 = 50.0).
    pub power: FixedScalar,
}

impl BrMaterial {
    /// Parse from a MTRLF chunk data slice (must be ≥ 20 bytes, bo=LE).
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < MTRLF_SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "MTRLF",
                need: MTRLF_SIZE,
                got: data.len(),
            });
        }

        let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
        if bo != BO_LITTLE_ENDIAN {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }

        // osk at [2..4] — not needed for rendering, skip
        let colour = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let ka     = u16::from_le_bytes(data[8..10].try_into().unwrap());
        let kd     = u16::from_le_bytes(data[10..12].try_into().unwrap());
        let ks     = u16::from_le_bytes(data[12..14].try_into().unwrap());
        let index_base  = data[14];
        let index_range = data[15];
        let power  = FixedScalar(i32::from_le_bytes(data[16..20].try_into().unwrap()));

        Ok(Self { colour, ka, kd, ks, index_base, index_range, power })
    }

    /// Serialize to 20 bytes (for round-trip testing).
    pub fn to_bytes(self) -> [u8; MTRLF_SIZE] {
        let mut b = [0u8; MTRLF_SIZE];
        b[0..2].copy_from_slice(&BO_LITTLE_ENDIAN.to_le_bytes());
        // osk at [2..4] left as zero
        b[4..8].copy_from_slice(&self.colour.to_le_bytes());
        b[8..10].copy_from_slice(&self.ka.to_le_bytes());
        b[10..12].copy_from_slice(&self.kd.to_le_bytes());
        b[12..14].copy_from_slice(&self.ks.to_le_bytes());
        b[14] = self.index_base;
        b[15] = self.index_range;
        b[16..20].copy_from_slice(&self.power.0.to_le_bytes());
        b
    }

    /// Extract red channel (0–255).
    pub fn red(&self) -> u8 {
        ((self.colour >> 16) & 0xFF) as u8
    }

    /// Extract green channel (0–255).
    pub fn green(&self) -> u8 {
        ((self.colour >> 8) & 0xFF) as u8
    }

    /// Extract blue channel (0–255).
    pub fn blue(&self) -> u8 {
        (self.colour & 0xFF) as u8
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_material() -> BrMaterial {
        BrMaterial {
            colour: 0x00_FF_80_40, // R=255, G=128, B=64
            ka: 6553,              // ≈0.10 (BR_UFRACTION(0.10))
            kd: 39321,             // ≈0.60
            ks: 39321,             // ≈0.60
            index_base: 0,
            index_range: 0,
            power: FixedScalar(0x0032_0000), // 50.0
        }
    }

    #[test]
    fn size_is_20() {
        assert_eq!(MTRLF_SIZE, 20);
    }

    #[test]
    fn roundtrip() {
        let mat = sample_material();
        let bytes = mat.to_bytes();
        assert_eq!(bytes.len(), MTRLF_SIZE);
        let mat2 = BrMaterial::from_bytes(&bytes).unwrap();
        assert_eq!(mat, mat2);
    }

    #[test]
    fn byte_order_marker_written() {
        let mat = sample_material();
        let bytes = mat.to_bytes();
        let bo = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(bo, 0x0001);
    }

    #[test]
    fn rejects_big_endian_bo() {
        let mut bytes = sample_material().to_bytes();
        // i16::from_le_bytes([0x00, 0x01]) = 0x0100 — not a valid LE marker
        bytes[0] = 0x00;
        bytes[1] = 0x01;
        let err = BrMaterial::from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, EngineError::InvalidByteOrder(0x0100)));
    }

    #[test]
    fn rejects_truncated_data() {
        let bytes = sample_material().to_bytes();
        let err = BrMaterial::from_bytes(&bytes[..15]).unwrap_err();
        assert!(matches!(err, EngineError::UnexpectedEof { .. }));
    }

    #[test]
    fn colour_channels() {
        let mat = sample_material();
        assert_eq!(mat.red(), 255);
        assert_eq!(mat.green(), 128);
        assert_eq!(mat.blue(), 64);
    }

    #[test]
    fn ka_fraction_range() {
        let mat = sample_material();
        let ka_f32 = mat.ka as f32 / 65535.0;
        assert!((ka_f32 - 0.10).abs() < 0.002, "ka={ka_f32}");
    }

    #[test]
    fn kd_fraction_range() {
        let mat = sample_material();
        let kd_f32 = mat.kd as f32 / 65535.0;
        assert!((kd_f32 - 0.60).abs() < 0.002, "kd={kd_f32}");
    }

    #[test]
    fn power_fixed_point() {
        let mat = sample_material();
        let power_f32 = mat.power.to_f64() as f32;
        assert!((power_f32 - 50.0).abs() < 0.01, "power={power_f32}");
    }

    #[test]
    fn zero_colour_parses() {
        let mat = BrMaterial {
            colour: 0,
            ka: 0,
            kd: 0,
            ks: 0,
            index_base: 0,
            index_range: 0,
            power: FixedScalar(0),
        };
        let bytes = mat.to_bytes();
        let mat2 = BrMaterial::from_bytes(&bytes).unwrap();
        assert_eq!(mat, mat2);
    }

    #[test]
    fn white_colour() {
        let mat = BrMaterial {
            colour: 0x00_FF_FF_FF,
            ka: 65535,
            kd: 65535,
            ks: 65535,
            index_base: 0,
            index_range: 0,
            power: FixedScalar(0x0001_0000), // 1.0
        };
        assert_eq!(mat.red(), 255);
        assert_eq!(mat.green(), 255);
        assert_eq!(mat.blue(), 255);
        let ka_f32 = mat.ka as f32 / 65535.0;
        assert!((ka_f32 - 1.0).abs() < 1e-4);
    }
}
