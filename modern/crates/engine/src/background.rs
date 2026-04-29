//! BRender background domain types: BKGDF, CAM (camera), LITE (light), GLLT.
//!
//! On-disk layouts:
//!
//!   BKGDF (8 bytes):
//!     [bo:i16][osk:i16][bIndexBase:u8][bPad:u8][swPad:i16]
//!
//!   CAM (76 bytes):
//!     [bo:i16][osk:i16][zrHither:BRS][zrYon:BRS][aFov:BRA][swPad:i16]
//!     [APOS:12][BMAT34:48]
//!
//!   LITE (56 bytes):
//!     [BMAT34:48][rIntensity:BRS][lt:i32]
//!
//!   GLLT / GLF header (12 bytes) + LITE entries:
//!     [bo:i16][osk:i16][cbEntry:i32][ivMac:i32][LITE × ivMac]
//!
//! Type notes:
//!   BRS  = i32 16.16 signed fixed-point (same as FixedScalar)
//!   BRA  = u16 angle: full circle = 65536 units = 2π radians
//!   BMAT34 = m[4][3] (4 rows × 3 cols) of BRS; row 3 = translation

use crate::error::{EngineError, EngineResult};
use crate::fixedpoint::FixedScalar;

// ── Constants ───────────────────────────────────────────────────────────────

const BO_LITTLE_ENDIAN: i16 = 0x0001;

pub const BKGDF_SIZE: usize = 8;
pub const CAM_SIZE: usize = 76;
pub const LITE_SIZE: usize = 56;
pub const GLF_HEADER_SIZE: usize = 12;

// ── BRA → radians ───────────────────────────────────────────────────────────

/// Convert a BRender angle (BRA u16, full circle = 65536) to radians.
pub fn bra_to_radians(bra: u16) -> f32 {
    bra as f32 / 65536.0 * std::f32::consts::TAU
}

// ── Bmat34 — 48 bytes ───────────────────────────────────────────────────────
//
// BRender BMAT34 is m[4][3]: 4 rows × 3 cols, row-major, each entry BRS.
// Row 0: x-axis (right vector)
// Row 1: y-axis (up vector)
// Row 2: z-axis (forward / camera direction)
// Row 3: translation

/// BRender 4×3 matrix. 48 bytes on disk (12 × i32 fixed-point, row-major).
///
/// Note: this differs from `transform::Mat34` (3×4). BMAT34 uses 4 rows × 3 cols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bmat34 {
    /// Row-major storage: m\[row\]\[col\], 4 rows × 3 cols.
    pub m: [[FixedScalar; 3]; 4],
}

impl Bmat34 {
    pub const SIZE: usize = 48;

    pub const IDENTITY: Self = Self {
        m: [
            [FixedScalar::ONE, FixedScalar::ZERO, FixedScalar::ZERO],
            [FixedScalar::ZERO, FixedScalar::ONE, FixedScalar::ZERO],
            [FixedScalar::ZERO, FixedScalar::ZERO, FixedScalar::ONE],
            [FixedScalar::ZERO, FixedScalar::ZERO, FixedScalar::ZERO],
        ],
    };

    pub fn from_le_bytes(b: &[u8; 48]) -> Self {
        let mut m = [[FixedScalar::ZERO; 3]; 4];
        for row in 0..4 {
            for col in 0..3 {
                let off = (row * 3 + col) * 4;
                m[row][col] = FixedScalar(i32::from_le_bytes(b[off..off + 4].try_into().unwrap()));
            }
        }
        Self { m }
    }

    pub fn to_le_bytes(self) -> [u8; 48] {
        let mut b = [0u8; 48];
        for row in 0..4 {
            for col in 0..3 {
                let off = (row * 3 + col) * 4;
                b[off..off + 4].copy_from_slice(&self.m[row][col].0.to_le_bytes());
            }
        }
        b
    }

    /// Extract translation (row 3, cols 0-2) as f32 values.
    pub fn translation_f32(&self) -> [f32; 3] {
        [
            self.m[3][0].0 as f32 / 65536.0,
            self.m[3][1].0 as f32 / 65536.0,
            self.m[3][2].0 as f32 / 65536.0,
        ]
    }

    /// Extract forward / z-axis direction (row 2, cols 0-2) as f32 values.
    pub fn forward_f32(&self) -> [f32; 3] {
        [
            self.m[2][0].0 as f32 / 65536.0,
            self.m[2][1].0 as f32 / 65536.0,
            self.m[2][2].0 as f32 / 65536.0,
        ]
    }
}

// ── Apos — 12 bytes ─────────────────────────────────────────────────────────

/// Actor placement point. 12 bytes on disk (3 × BRS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Apos {
    pub xr_place: FixedScalar,
    pub yr_place: FixedScalar,
    pub zr_place: FixedScalar,
}

impl Apos {
    pub const SIZE: usize = 12;

    pub fn from_le_bytes(b: &[u8; 12]) -> Self {
        Self {
            xr_place: FixedScalar(i32::from_le_bytes(b[0..4].try_into().unwrap())),
            yr_place: FixedScalar(i32::from_le_bytes(b[4..8].try_into().unwrap())),
            zr_place: FixedScalar(i32::from_le_bytes(b[8..12].try_into().unwrap())),
        }
    }

    pub fn to_le_bytes(self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0..4].copy_from_slice(&self.xr_place.0.to_le_bytes());
        b[4..8].copy_from_slice(&self.yr_place.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.zr_place.0.to_le_bytes());
        b
    }
}

// ── BrBackground (BKGDF) — 8 bytes ─────────────────────────────────────────

/// BRender background header as stored on disk (BKGDF). 8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrBackground {
    /// Palette index base for this background's colour palette.
    pub b_index_base: u8,
}

impl BrBackground {
    /// Parse from a BKGDF chunk data slice (must be ≥ 8 bytes, bo=LE).
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < BKGDF_SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "BKGDF",
                need: BKGDF_SIZE,
                got: data.len(),
            });
        }
        let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
        if bo != BO_LITTLE_ENDIAN {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }
        Ok(Self {
            b_index_base: data[4],
        })
    }

    /// Serialize to 8 bytes (for round-trip testing).
    pub fn to_bytes(self) -> [u8; BKGDF_SIZE] {
        let mut b = [0u8; BKGDF_SIZE];
        b[0..2].copy_from_slice(&BO_LITTLE_ENDIAN.to_le_bytes());
        b[4] = self.b_index_base;
        b
    }
}

// ── BrCamera (CAM) — 76 bytes ────────────────────────────────────────────────

/// BRender camera as stored on disk (CAM chunk). 76 bytes.
///
/// Conversion notes:
/// - `hither_z` / `yon_z`: BRS → f32 by dividing by 65536.0
/// - `a_fov`: BRA → radians via `bra_to_radians()`
/// - `bmat34`: row 3 = position, row 2 = forward direction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrCamera {
    /// Near clipping plane (BRS 16.16 fixed-point).
    pub hither_z: FixedScalar,
    /// Far clipping plane (BRS 16.16 fixed-point).
    pub yon_z: FixedScalar,
    /// Vertical field of view (BRA u16: 0..65535 → 0..2π).
    pub a_fov: u16,
    /// Actor placement point for this camera view.
    pub apos: Apos,
    /// Camera view matrix (4×3 BRS, row 3 = position, row 2 = forward).
    pub bmat34: Bmat34,
}

impl BrCamera {
    /// Parse from a CAM chunk data slice (must be ≥ 76 bytes, bo=LE).
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < CAM_SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "CAM",
                need: CAM_SIZE,
                got: data.len(),
            });
        }
        let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
        if bo != BO_LITTLE_ENDIAN {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }
        Ok(Self {
            hither_z: FixedScalar(i32::from_le_bytes(data[4..8].try_into().unwrap())),
            yon_z: FixedScalar(i32::from_le_bytes(data[8..12].try_into().unwrap())),
            a_fov: u16::from_le_bytes(data[12..14].try_into().unwrap()),
            // swPad at [14..16] — skipped
            apos: Apos::from_le_bytes(data[16..28].try_into().unwrap()),
            bmat34: Bmat34::from_le_bytes(data[28..76].try_into().unwrap()),
        })
    }

    /// Serialize to 76 bytes (for round-trip testing).
    pub fn to_bytes(&self) -> [u8; CAM_SIZE] {
        let mut b = [0u8; CAM_SIZE];
        b[0..2].copy_from_slice(&BO_LITTLE_ENDIAN.to_le_bytes());
        b[4..8].copy_from_slice(&self.hither_z.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.yon_z.0.to_le_bytes());
        b[12..14].copy_from_slice(&self.a_fov.to_le_bytes());
        b[16..28].copy_from_slice(&self.apos.to_le_bytes());
        b[28..76].copy_from_slice(&self.bmat34.to_le_bytes());
        b
    }

    /// Return field of view as radians (converts BRA u16).
    pub fn fov_radians(&self) -> f32 {
        bra_to_radians(self.a_fov)
    }

    /// Return near plane as f32 (BRS → float).
    pub fn hither_f32(&self) -> f32 {
        self.hither_z.0 as f32 / 65536.0
    }

    /// Return far plane as f32 (BRS → float).
    pub fn yon_f32(&self) -> f32 {
        self.yon_z.0 as f32 / 65536.0
    }
}

// ── BrLight (LITE) — 56 bytes ────────────────────────────────────────────────

/// Light type constants matching the C++ `lt` enum.
pub mod light_type {
    pub const DIRECTIONAL: i32 = 1;
    pub const POINT: i32 = 2;
    pub const SPOT: i32 = 4;
    pub const AMBIENT: i32 = 8;
}

/// BRender light as stored on disk (LITE struct). 56 bytes.
///
/// No byte-order marker — LITE entries are embedded inside a GLLT chunk
/// whose bo is validated at the GLF header level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrLight {
    /// Position/orientation matrix (row 2 = light direction).
    pub bmat34: Bmat34,
    /// Light intensity (BRS 16.16 fixed-point).
    pub r_intensity: FixedScalar,
    /// Light type (see [`light_type`] constants).
    pub lt: i32,
}

impl BrLight {
    /// Parse from a LITE data slice (must be ≥ 56 bytes).
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < LITE_SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "LITE",
                need: LITE_SIZE,
                got: data.len(),
            });
        }
        Ok(Self {
            bmat34: Bmat34::from_le_bytes(data[0..48].try_into().unwrap()),
            r_intensity: FixedScalar(i32::from_le_bytes(data[48..52].try_into().unwrap())),
            lt: i32::from_le_bytes(data[52..56].try_into().unwrap()),
        })
    }

    /// Serialize to 56 bytes (for round-trip testing).
    pub fn to_bytes(&self) -> [u8; LITE_SIZE] {
        let mut b = [0u8; LITE_SIZE];
        b[0..48].copy_from_slice(&self.bmat34.to_le_bytes());
        b[48..52].copy_from_slice(&self.r_intensity.0.to_le_bytes());
        b[52..56].copy_from_slice(&self.lt.to_le_bytes());
        b
    }

    /// Return intensity as f32 (BRS → float).
    pub fn intensity_f32(&self) -> f32 {
        self.r_intensity.0 as f32 / 65536.0
    }
}

// ── BrLightList (GLLT) ───────────────────────────────────────────────────────

/// GL-of-lights list from a GLLT chunk: 12-byte GLF header + LITE entries.
#[derive(Debug, Clone)]
pub struct BrLightList {
    pub lights: Vec<BrLight>,
}

impl BrLightList {
    /// Parse from a GLLT chunk data slice.
    ///
    /// GLF header: [bo:i16][osk:i16][cbEntry:i32][ivMac:i32]
    /// Followed by `ivMac` × LITE entries (56 bytes each).
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < GLF_HEADER_SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "GLLT header",
                need: GLF_HEADER_SIZE,
                got: data.len(),
            });
        }
        let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
        if bo != BO_LITTLE_ENDIAN {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }
        let cb_entry = i32::from_le_bytes(data[4..8].try_into().unwrap());
        let iv_mac = i32::from_le_bytes(data[8..12].try_into().unwrap());

        if cb_entry != LITE_SIZE as i32 {
            return Err(EngineError::OutOfRange {
                what: "GLLT cbEntry",
                value: cb_entry as i64,
            });
        }

        let count = iv_mac.max(0) as usize;
        let required = GLF_HEADER_SIZE + count * LITE_SIZE;
        if data.len() < required {
            return Err(EngineError::UnexpectedEof {
                what: "GLLT entries",
                need: required,
                got: data.len(),
            });
        }

        let mut lights = Vec::with_capacity(count);
        for i in 0..count {
            let off = GLF_HEADER_SIZE + i * LITE_SIZE;
            lights.push(BrLight::from_bytes(&data[off..off + LITE_SIZE])?);
        }

        Ok(Self { lights })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Bmat34 ───────────────────────────────────────────────────────────────

    #[test]
    fn bmat34_size_is_48() {
        assert_eq!(Bmat34::SIZE, 48);
    }

    #[test]
    fn bmat34_identity_roundtrip() {
        let bytes = Bmat34::IDENTITY.to_le_bytes();
        assert_eq!(bytes.len(), 48);
        let m2 = Bmat34::from_le_bytes(&bytes);
        assert_eq!(m2, Bmat34::IDENTITY);
    }

    #[test]
    fn bmat34_translation_extracted() {
        let mut mat = Bmat34::IDENTITY;
        mat.m[3][0] = FixedScalar(0x0003_0000); // 3.0
        mat.m[3][1] = FixedScalar(0x0004_0000); // 4.0
        mat.m[3][2] = FixedScalar(0x0005_0000); // 5.0
        let t = mat.translation_f32();
        assert!((t[0] - 3.0).abs() < 1e-5, "x={}", t[0]);
        assert!((t[1] - 4.0).abs() < 1e-5, "y={}", t[1]);
        assert!((t[2] - 5.0).abs() < 1e-5, "z={}", t[2]);
    }

    #[test]
    fn bmat34_forward_extracted() {
        let mut mat = Bmat34::IDENTITY;
        // Row 2 = forward: set to (0.0, 0.0, 1.0) = identity z-axis
        let fwd = mat.forward_f32();
        assert!((fwd[0]).abs() < 1e-5);
        assert!((fwd[1]).abs() < 1e-5);
        assert!((fwd[2] - 1.0).abs() < 1e-5);
        // Rotate forward to (1.0, 0.0, 0.0)
        mat.m[2][0] = FixedScalar::ONE;
        mat.m[2][2] = FixedScalar::ZERO;
        let fwd2 = mat.forward_f32();
        assert!((fwd2[0] - 1.0).abs() < 1e-5);
        assert!((fwd2[2]).abs() < 1e-5);
    }

    // ── BRA ──────────────────────────────────────────────────────────────────

    #[test]
    fn bra_zero_is_zero_radians() {
        assert_eq!(bra_to_radians(0), 0.0);
    }

    #[test]
    fn bra_half_circle() {
        // 0x8000 = 32768 → π
        let r = bra_to_radians(0x8000);
        assert!((r - std::f32::consts::PI).abs() < 1e-5, "r={r}");
    }

    #[test]
    fn bra_quarter_circle() {
        // 0x4000 = 16384 → π/2
        let r = bra_to_radians(0x4000);
        assert!((r - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "r={r}");
    }

    // ── BrBackground ─────────────────────────────────────────────────────────

    #[test]
    fn background_size_is_8() {
        assert_eq!(BKGDF_SIZE, 8);
    }

    #[test]
    fn background_roundtrip() {
        let bkg = BrBackground { b_index_base: 7 };
        let bytes = bkg.to_bytes();
        assert_eq!(bytes.len(), BKGDF_SIZE);
        let bkg2 = BrBackground::from_bytes(&bytes).unwrap();
        assert_eq!(bkg, bkg2);
    }

    #[test]
    fn background_rejects_bad_bo() {
        let mut bytes = BrBackground { b_index_base: 0 }.to_bytes();
        bytes[0] = 0x00;
        bytes[1] = 0x01; // i16::from_le_bytes([0x00,0x01]) = 0x0100 ≠ 0x0001
        let err = BrBackground::from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, EngineError::InvalidByteOrder(0x0100)));
    }

    // ── BrCamera ─────────────────────────────────────────────────────────────

    #[test]
    fn camera_size_is_76() {
        assert_eq!(CAM_SIZE, 76);
    }

    fn sample_camera() -> BrCamera {
        BrCamera {
            hither_z: FixedScalar(0x0000_1999), // ~0.1
            yon_z: FixedScalar(0x0064_0000),    // 100.0
            a_fov: 0x2000,                      // ~45° = π/4
            apos: Apos {
                xr_place: FixedScalar::ZERO,
                yr_place: FixedScalar::ZERO,
                zr_place: FixedScalar::ZERO,
            },
            bmat34: Bmat34::IDENTITY,
        }
    }

    #[test]
    fn camera_roundtrip() {
        let cam = sample_camera();
        let bytes = cam.to_bytes();
        assert_eq!(bytes.len(), CAM_SIZE);
        let cam2 = BrCamera::from_bytes(&bytes).unwrap();
        assert_eq!(cam, cam2);
    }

    #[test]
    fn camera_fov_conversion() {
        // a_fov = 0x2000 = 8192, bra_to_radians(8192) = 8192/65536*TAU ≈ π/4
        let cam = sample_camera();
        let fov = cam.fov_radians();
        assert!(
            (fov - std::f32::consts::FRAC_PI_4).abs() < 1e-4,
            "fov={fov}"
        );
    }

    #[test]
    fn camera_hither_yon_conversion() {
        let cam = sample_camera();
        assert!(
            (cam.hither_f32() - 0.1).abs() < 0.002,
            "hither={}",
            cam.hither_f32()
        );
        assert!(
            (cam.yon_f32() - 100.0).abs() < 0.01,
            "yon={}",
            cam.yon_f32()
        );
    }

    #[test]
    fn camera_rejects_truncated() {
        let bytes = sample_camera().to_bytes();
        let err = BrCamera::from_bytes(&bytes[..40]).unwrap_err();
        assert!(matches!(err, EngineError::UnexpectedEof { .. }));
    }

    // ── BrLight ──────────────────────────────────────────────────────────────

    #[test]
    fn lite_size_is_56() {
        assert_eq!(LITE_SIZE, 56);
    }

    fn sample_light() -> BrLight {
        BrLight {
            bmat34: Bmat34::IDENTITY,
            r_intensity: FixedScalar(0x0001_0000), // 1.0
            lt: light_type::DIRECTIONAL,
        }
    }

    #[test]
    fn lite_roundtrip() {
        let lit = sample_light();
        let bytes = lit.to_bytes();
        assert_eq!(bytes.len(), LITE_SIZE);
        let lit2 = BrLight::from_bytes(&bytes).unwrap();
        assert_eq!(lit, lit2);
    }

    #[test]
    fn lite_intensity_conversion() {
        let lit = sample_light();
        assert!((lit.intensity_f32() - 1.0).abs() < 1e-5);
    }

    // ── BrLightList ──────────────────────────────────────────────────────────

    #[test]
    fn light_list_roundtrip() {
        // Manually construct a 1-light GLLT chunk.
        let lit = sample_light();
        let lite_bytes = lit.to_bytes();

        let mut data = vec![0u8; GLF_HEADER_SIZE + LITE_SIZE];
        // GLF header: bo=1, osk=0, cbEntry=56, ivMac=1
        data[0..2].copy_from_slice(&1i16.to_le_bytes());
        data[4..8].copy_from_slice(&(LITE_SIZE as i32).to_le_bytes());
        data[8..12].copy_from_slice(&1i32.to_le_bytes());
        data[12..].copy_from_slice(&lite_bytes);

        let list = BrLightList::from_bytes(&data).unwrap();
        assert_eq!(list.lights.len(), 1);
        assert_eq!(list.lights[0], lit);
    }

    #[test]
    fn light_list_rejects_bad_entry_size() {
        let mut data = vec![0u8; GLF_HEADER_SIZE];
        data[0..2].copy_from_slice(&1i16.to_le_bytes());
        data[4..8].copy_from_slice(&32i32.to_le_bytes()); // wrong cbEntry
        data[8..12].copy_from_slice(&0i32.to_le_bytes());
        let err = BrLightList::from_bytes(&data).unwrap_err();
        assert!(matches!(err, EngineError::OutOfRange { .. }));
    }
}
