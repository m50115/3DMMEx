//! BRender fixed-point math types.
//!
//! BRS = 16.16 signed fixed-point (i32)
//! BRA = unsigned angle (u16, 0-65535 = 0-360 degrees)

/// BRender scalar — 16.16 signed fixed-point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FixedScalar(pub i32);

/// BRender angle — 0-65535 maps to 0-360 degrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedAngle(pub u16);

impl FixedScalar {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(0x0001_0000); // 1.0 in 16.16

    /// Convert from float (for display/calculation only — NEVER use in serialization)
    pub fn from_f64(v: f64) -> Self {
        Self((v * 65536.0) as i32)
    }

    /// Convert to float (for display/calculation only)
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 65536.0
    }

    /// Raw i32 value — use this for serialization
    pub fn raw(self) -> i32 {
        self.0
    }
}

impl FixedAngle {
    pub const ZERO: Self = Self(0);

    /// Convert to degrees
    pub fn to_degrees(self) -> f64 {
        self.0 as f64 * 360.0 / 65536.0
    }

    /// Convert from degrees
    pub fn from_degrees(deg: f64) -> Self {
        Self((deg * 65536.0 / 360.0) as u16)
    }

    /// Raw u16 value — use this for serialization
    pub fn raw(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_scalar_one() {
        assert_eq!(FixedScalar::ONE.to_f64(), 1.0);
    }

    #[test]
    fn test_fixed_scalar_roundtrip() {
        let v = FixedScalar::from_f64(3.14);
        // Not exact due to fixed-point, but close
        assert!((v.to_f64() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_fixed_angle_degrees() {
        let a = FixedAngle::from_degrees(90.0);
        assert!((a.to_degrees() - 90.0).abs() < 0.01);
    }

    #[test]
    fn test_fixed_scalar_raw_preserved() {
        let original = 0x0002_8000i32; // 2.5 in 16.16
        let s = FixedScalar(original);
        assert_eq!(s.raw(), original);
    }
}
