//! Movie domain model — MVIE chunk and MFP on-disk header.
//!
//! MFP (Movie File Prefix) — 8 bytes:
//!   [i16 bo][i16 osk][i16 ver_cur][i16 ver_back]
//!
//! Current version: 2. Minimum backward-compatible: 2.
//!
//! The MVIE chunk contains:
//!   - MFP header bytes as chunk data
//!   - SCEN sub-chunks (chid = scene index, 0-based)
//!   - GST roll-call chunk (chid = CHID_GST_MACTR)
//!   - MSND sound chunks

use crate::error::{EngineError, EngineResult};
use crate::scene::Scene;

const BO_LE: i16 = 0x0001;

/// MFP version constants.
pub const MFP_VER_CUR: i16  = 2;
pub const MFP_VER_BACK: i16 = 2;

// ── MFP header — 8 bytes ─────────────────────────────────────────────────────

/// Movie file prefix as stored at the start of the MVIE chunk data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieFilePrefix {
    pub bo: i16,
    pub osk: i16,
    pub ver_cur: i16,
    pub ver_back: i16,
}

impl MovieFilePrefix {
    pub const SIZE: usize = 8;

    pub fn from_bytes(b: &[u8; 8]) -> EngineResult<Self> {
        let bo_raw = i16::from_le_bytes(b[0..2].try_into().unwrap());
        let swap = !matches!(bo_raw, BO_LE);

        let read_i16 = |off: usize| -> i16 {
            let raw = i16::from_le_bytes(b[off..off + 2].try_into().unwrap());
            if swap { raw.swap_bytes() } else { raw }
        };

        let bo       = read_i16(0);
        let osk      = read_i16(2);
        let ver_cur  = read_i16(4);
        let ver_back = read_i16(6);

        if bo != BO_LE {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }
        if ver_back > MFP_VER_CUR {
            return Err(EngineError::UnsupportedVersion { cur: ver_cur, back: ver_back });
        }

        Ok(Self { bo: bo_raw, osk, ver_cur, ver_back })
    }

    pub fn to_le_bytes(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..2].copy_from_slice(&self.bo.to_le_bytes());
        b[2..4].copy_from_slice(&self.osk.to_le_bytes());
        b[4..6].copy_from_slice(&self.ver_cur.to_le_bytes());
        b[6..8].copy_from_slice(&self.ver_back.to_le_bytes());
        b
    }

    /// Create a new LE/Windows MFP header with current version.
    pub fn new_le() -> Self {
        Self {
            bo:       BO_LE,
            osk:      0x7769, // Windows
            ver_cur:  MFP_VER_CUR,
            ver_back: MFP_VER_BACK,
        }
    }
}

// ── Runtime Movie ─────────────────────────────────────────────────────────────

/// Runtime movie — fully deserialized from a MVIE chunk tree.
#[derive(Debug, Clone)]
pub struct Movie {
    /// Scenes in order (scene 0 = first).
    pub scenes: Vec<Scene>,
}

impl Movie {
    pub fn new(scenes: Vec<Scene>) -> Self {
        Self { scenes }
    }

    /// Total frames across all scenes.
    pub fn total_frames(&self) -> i32 {
        self.scenes.iter().map(|s| s.nfrm_mac).sum()
    }

    /// Number of scenes.
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mfp(bo: i16, ver_cur: i16, ver_back: i16) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..2].copy_from_slice(&bo.to_le_bytes());
        b[2..4].copy_from_slice(&0x7769i16.to_le_bytes());
        b[4..6].copy_from_slice(&ver_cur.to_le_bytes());
        b[6..8].copy_from_slice(&ver_back.to_le_bytes());
        b
    }

    #[test]
    fn test_mfp_size() {
        assert_eq!(MovieFilePrefix::SIZE, 8);
    }

    #[test]
    fn test_mfp_parse_current_version() {
        let bytes = make_mfp(BO_LE, MFP_VER_CUR, MFP_VER_BACK);
        let mfp = MovieFilePrefix::from_bytes(&bytes).unwrap();
        assert_eq!(mfp.ver_cur, 2);
        assert_eq!(mfp.ver_back, 2);
    }

    #[test]
    fn test_mfp_unsupported_version() {
        let bytes = make_mfp(BO_LE, 5, 5); // ver_back > MFP_VER_CUR
        let result = MovieFilePrefix::from_bytes(&bytes);
        assert!(matches!(result, Err(EngineError::UnsupportedVersion { .. })));
    }

    #[test]
    fn test_mfp_roundtrip() {
        let bytes = make_mfp(BO_LE, 2, 2);
        let mfp = MovieFilePrefix::from_bytes(&bytes).unwrap();
        let rt = mfp.to_le_bytes();
        assert_eq!(bytes, rt);
    }

    #[test]
    fn test_mfp_new_le() {
        let mfp = MovieFilePrefix::new_le();
        assert_eq!(mfp.bo, BO_LE);
        assert_eq!(mfp.ver_cur, MFP_VER_CUR);
        let bytes = mfp.to_le_bytes();
        let parsed = MovieFilePrefix::from_bytes(&bytes).unwrap();
        assert_eq!(mfp, parsed);
    }

    #[test]
    fn test_movie_total_frames() {
        use crate::scene::{Scene, SceneHeader};

        let make_scene = |nfrm_mac: i32| {
            let mut b = [0u8; 16];
            b[0..2].copy_from_slice(&BO_LE.to_le_bytes());
            b[2..4].copy_from_slice(&0x7769i16.to_le_bytes());
            b[8..12].copy_from_slice(&nfrm_mac.to_le_bytes());
            let hdr = SceneHeader::from_bytes(&b).unwrap();
            Scene::new(hdr, vec![])
        };

        let movie = Movie::new(vec![make_scene(30), make_scene(60), make_scene(45)]);
        assert_eq!(movie.total_frames(), 135);
        assert_eq!(movie.scene_count(), 3);
    }
}
