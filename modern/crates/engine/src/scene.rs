//! Scene domain model — SCEN chunk and SCENH on-disk header.
//!
//! SCENH (Scene Header) — 16 bytes:
//!   [i16 bo][i16 osk][i32 nfrm_cur][i32 nfrm_mac][i32 unused]
//!
//! A SCEN chunk contains:
//!   - The SCENH header bytes as chunk data
//!   - ACTR sub-chunks (one per actor, chid = actor index)
//!   - TBOX sub-chunks (text boxes)

use crate::actor::Actor;
use crate::error::{EngineError, EngineResult};

// ── SCENH — 16 bytes on disk ──────────────────────────────────────────────────

const BO_LE: i16 = 0x0001;

/// Scene header as stored on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneHeader {
    pub bo: i16,
    pub osk: i16,
    /// Current frame when the scene was saved (playback position).
    pub nfrm_cur: i32,
    /// Total number of frames in the scene.
    pub nfrm_mac: i32,
    /// Unused padding field.
    pub unused: i32,
}

impl SceneHeader {
    pub const SIZE: usize = 16;

    pub fn from_bytes(b: &[u8; 16]) -> EngineResult<Self> {
        let bo_raw = i16::from_le_bytes(b[0..2].try_into().unwrap());
        let swap = !matches!(bo_raw, BO_LE);

        let read_i16 = |off: usize| -> i16 {
            let raw = i16::from_le_bytes(b[off..off + 2].try_into().unwrap());
            if swap { raw.swap_bytes() } else { raw }
        };
        let read_i32 = |off: usize| -> i32 {
            let raw = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
            if swap { raw.swap_bytes() } else { raw }
        };

        let bo  = read_i16(0);
        let osk = read_i16(2);
        if bo != BO_LE {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }

        Ok(Self {
            bo:  bo_raw,
            osk: osk as i16,
            nfrm_cur: read_i32(4),
            nfrm_mac: read_i32(8),
            unused:   read_i32(12),
        })
    }

    pub fn to_le_bytes(&self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..2].copy_from_slice(&self.bo.to_le_bytes());
        b[2..4].copy_from_slice(&self.osk.to_le_bytes());
        b[4..8].copy_from_slice(&self.nfrm_cur.to_le_bytes());
        b[8..12].copy_from_slice(&self.nfrm_mac.to_le_bytes());
        b[12..16].copy_from_slice(&self.unused.to_le_bytes());
        b
    }
}

// ── Runtime Scene ─────────────────────────────────────────────────────────────

/// Runtime scene — fully deserialized from a SCEN chunk tree.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Current playback frame (from SCENH).
    pub nfrm_cur: i32,
    /// Total frames in this scene.
    pub nfrm_mac: i32,
    /// Actors in this scene (ordered by chid).
    pub actors: Vec<Actor>,
}

impl Scene {
    pub fn new(header: SceneHeader, actors: Vec<Actor>) -> Self {
        Self {
            nfrm_cur: header.nfrm_cur,
            nfrm_mac: header.nfrm_mac,
            actors,
        }
    }

    /// Find an actor by its `arid`.
    pub fn actor_by_id(&self, arid: i32) -> Option<&Actor> {
        self.actors.iter().find(|a| a.arid == arid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scenh(nfrm_cur: i32, nfrm_mac: i32) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..2].copy_from_slice(&(BO_LE as i16).to_le_bytes());
        b[2..4].copy_from_slice(&(0x7769i16).to_le_bytes()); // osk Windows
        b[4..8].copy_from_slice(&nfrm_cur.to_le_bytes());
        b[8..12].copy_from_slice(&nfrm_mac.to_le_bytes());
        // unused = 0
        b
    }

    #[test]
    fn test_scene_header_size() {
        assert_eq!(SceneHeader::SIZE, 16);
    }

    #[test]
    fn test_scene_header_parse() {
        let bytes = make_scenh(3, 60);
        let hdr = SceneHeader::from_bytes(&bytes).unwrap();
        assert_eq!(hdr.nfrm_cur, 3);
        assert_eq!(hdr.nfrm_mac, 60);
    }

    #[test]
    fn test_scene_header_roundtrip() {
        let bytes = make_scenh(7, 120);
        let hdr = SceneHeader::from_bytes(&bytes).unwrap();
        let rt = hdr.to_le_bytes();
        assert_eq!(bytes, rt);
    }

    #[test]
    fn test_scene_actor_lookup() {
        use crate::actor::{Actor, ActorOnFile};
        use crate::tag::CTG_ACTR;

        // Build a minimal ActorOnFile
        let mut b = [0u8; 44];
        b[0..2].copy_from_slice(&(BO_LE as i16).to_le_bytes());
        b[2..4].copy_from_slice(&0x7769i16.to_le_bytes());
        b[16..20].copy_from_slice(&99i32.to_le_bytes()); // arid = 99
        b[36..40].copy_from_slice(&CTG_ACTR.to_le_bytes());
        let actf = ActorOnFile::from_bytes(&b).unwrap();
        let actor = Actor::new(actf, vec![], vec![]);

        let hdr = SceneHeader::from_bytes(&make_scenh(0, 30)).unwrap();
        let scene = Scene::new(hdr, vec![actor]);

        assert!(scene.actor_by_id(99).is_some());
        assert!(scene.actor_by_id(0).is_none());
    }
}
