//! 3D transform types: Vec3, RoutePoint, RouteLocation, Mat34.
//!
//! All values are BRS (i32 16.16 fixed-point). NEVER convert to float during
//! serialization — preserve raw i32 values exactly.

use crate::fixedpoint::FixedScalar;

// ── Vec3 — 12 bytes ─────────────────────────────────────────────────────────

/// 3D vector of BRS fixed-point scalars. 12 bytes on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vec3 {
    pub x: FixedScalar,
    pub y: FixedScalar,
    pub z: FixedScalar,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: FixedScalar::ZERO,
        y: FixedScalar::ZERO,
        z: FixedScalar::ZERO,
    };

    /// Parse from 12 raw LE bytes.
    pub fn from_le_bytes(b: &[u8; 12]) -> Self {
        Self {
            x: FixedScalar(i32::from_le_bytes(b[0..4].try_into().unwrap())),
            y: FixedScalar(i32::from_le_bytes(b[4..8].try_into().unwrap())),
            z: FixedScalar(i32::from_le_bytes(b[8..12].try_into().unwrap())),
        }
    }

    /// Serialize to 12 LE bytes.
    pub fn to_le_bytes(self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0..4].copy_from_slice(&self.x.0.to_le_bytes());
        b[4..8].copy_from_slice(&self.y.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.z.0.to_le_bytes());
        b
    }
}

// ── RoutePoint (RPT) — 16 bytes ──────────────────────────────────────────────
//
// Layout: [Vec3 position:12][i32 dwr:4]
// `dwr` is the cumulative path distance at this point (BRS fixed-point).

/// A single point on an actor's route path. 16 bytes on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutePoint {
    pub position: Vec3,
    /// Cumulative distance along the route (BRS fixed-point).
    pub dwr: FixedScalar,
}

impl RoutePoint {
    pub const SIZE: usize = 16;

    pub fn from_le_bytes(b: &[u8; 16]) -> Self {
        Self {
            position: Vec3::from_le_bytes(b[0..12].try_into().unwrap()),
            dwr: FixedScalar(i32::from_le_bytes(b[12..16].try_into().unwrap())),
        }
    }

    pub fn to_le_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..12].copy_from_slice(&self.position.to_le_bytes());
        b[12..16].copy_from_slice(&self.dwr.0.to_le_bytes());
        b
    }
}

// ── RouteLocation (RTEL) — 12 bytes ─────────────────────────────────────────
//
// Layout: [i32 irpt:4][i32 dwr:4][i32 dnwr:4]
// Locates an actor on the route: index of the segment + fractional distances.

/// Position of an actor on its route. 12 bytes on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteLocation {
    /// Index of the route segment (between irpt and irpt+1).
    pub irpt: i32,
    /// Distance from origin to this point along the full route (BRS).
    pub dwr: FixedScalar,
    /// Distance remaining to the next route point (BRS).
    pub dnwr: FixedScalar,
}

impl RouteLocation {
    pub const SIZE: usize = 12;

    pub fn from_le_bytes(b: &[u8; 12]) -> Self {
        Self {
            irpt: i32::from_le_bytes(b[0..4].try_into().unwrap()),
            dwr: FixedScalar(i32::from_le_bytes(b[4..8].try_into().unwrap())),
            dnwr: FixedScalar(i32::from_le_bytes(b[8..12].try_into().unwrap())),
        }
    }

    pub fn to_le_bytes(self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0..4].copy_from_slice(&self.irpt.to_le_bytes());
        b[4..8].copy_from_slice(&self.dwr.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.dnwr.0.to_le_bytes());
        b
    }
}

// ── Mat34 — 48 bytes ─────────────────────────────────────────────────────────
//
// BRender 3×4 matrix stored row-major:
//   [m[0][0..3]] [m[1][0..3]] [m[2][0..3]] [m[3][0..3]]
// Each element is a BRS i32 (16.16 fixed-point).
// Row 3 (m[3]) is the translation vector; col 3 is always (0,0,0,1) implicitly.

/// BRender 3×4 transform matrix. 48 bytes on disk (12 × i32 fixed-point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mat34 {
    /// Row-major storage: m[row][col], 3 rows × 4 columns.
    pub m: [[FixedScalar; 4]; 3],
}

impl Mat34 {
    pub const SIZE: usize = 48;

    /// Identity matrix (diagonal = 1.0, translation = 0).
    pub const IDENTITY: Self = Self {
        m: [
            [FixedScalar::ONE, FixedScalar::ZERO, FixedScalar::ZERO, FixedScalar::ZERO],
            [FixedScalar::ZERO, FixedScalar::ONE, FixedScalar::ZERO, FixedScalar::ZERO],
            [FixedScalar::ZERO, FixedScalar::ZERO, FixedScalar::ONE, FixedScalar::ZERO],
        ],
    };

    pub fn from_le_bytes(b: &[u8; 48]) -> Self {
        let mut m = [[FixedScalar::ZERO; 4]; 3];
        for row in 0..3 {
            for col in 0..4 {
                let off = (row * 4 + col) * 4;
                m[row][col] = FixedScalar(i32::from_le_bytes(b[off..off + 4].try_into().unwrap()));
            }
        }
        Self { m }
    }

    pub fn to_le_bytes(self) -> [u8; 48] {
        let mut b = [0u8; 48];
        for row in 0..3 {
            for col in 0..4 {
                let off = (row * 4 + col) * 4;
                b[off..off + 4].copy_from_slice(&self.m[row][col].0.to_le_bytes());
            }
        }
        b
    }

    /// Extract the translation vector (row 2, columns 0..2).
    pub fn translation(&self) -> Vec3 {
        Vec3 {
            x: self.m[2][0],
            y: self.m[2][1],
            z: self.m[2][2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec3_roundtrip() {
        let v = Vec3 {
            x: FixedScalar(0x0001_0000),
            y: FixedScalar(0x0002_0000),
            z: FixedScalar(-0x0001_8000),
        };
        let bytes = v.to_le_bytes();
        let v2 = Vec3::from_le_bytes(&bytes);
        assert_eq!(v, v2);
    }

    #[test]
    fn test_route_point_size() {
        // Ensure SIZE constant matches actual serialized output.
        let rpt = RoutePoint {
            position: Vec3::ZERO,
            dwr: FixedScalar::ZERO,
        };
        assert_eq!(rpt.to_le_bytes().len(), RoutePoint::SIZE);
    }

    #[test]
    fn test_route_location_roundtrip() {
        let rl = RouteLocation {
            irpt: 3,
            dwr: FixedScalar(0x0001_8000),
            dnwr: FixedScalar(0x0000_8000),
        };
        let bytes = rl.to_le_bytes();
        assert_eq!(bytes.len(), RouteLocation::SIZE);
        let rl2 = RouteLocation::from_le_bytes(&bytes);
        assert_eq!(rl, rl2);
    }

    #[test]
    fn test_mat34_identity_roundtrip() {
        let bytes = Mat34::IDENTITY.to_le_bytes();
        assert_eq!(bytes.len(), Mat34::SIZE);
        let m2 = Mat34::from_le_bytes(&bytes);
        assert_eq!(m2, Mat34::IDENTITY);
    }

    #[test]
    fn test_mat34_translation() {
        let mut id = Mat34::IDENTITY;
        id.m[2][0] = FixedScalar(0x0003_0000); // x = 3.0
        id.m[2][1] = FixedScalar(0x0004_0000); // y = 4.0
        id.m[2][2] = FixedScalar(0x0005_0000); // z = 5.0
        let t = id.translation();
        assert_eq!(t.x.to_f64(), 3.0);
        assert_eq!(t.y.to_f64(), 4.0);
        assert_eq!(t.z.to_f64(), 5.0);
    }

    #[test]
    fn test_raw_values_preserved() {
        // Serialization must NOT alter fixed-point values via float conversion.
        let raw_x = 0x0002_8000i32; // 2.5 in 16.16
        let v = Vec3 {
            x: FixedScalar(raw_x),
            y: FixedScalar::ZERO,
            z: FixedScalar::ZERO,
        };
        let bytes = v.to_le_bytes();
        assert_eq!(i32::from_le_bytes(bytes[0..4].try_into().unwrap()), raw_x);
    }
}
