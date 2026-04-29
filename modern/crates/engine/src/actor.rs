//! Actor domain model — ACTR chunk and ACTF on-disk format.
//!
//! ACTF (Actor on File) — 44 bytes:
//!   [i16 bo][i16 osk][Vec3 dxyz_full_rte:12][i32 arid][i32 nfrm_first][i32 nfrm_last][TagOnFile:16]
//!
//! After ACTF, the ACTR chunk contains two sub-chunks as children:
//!   - PATH (chid=0) — GL of RoutePoint entries
//!   - GGAE (chid=0) — GG of ActorEvent entries
//!
//! bo/osk determine the byte order for BE (Mac) files — LE (Windows) is the common case.

use crate::error::{EngineError, EngineResult};
use crate::events::ActorEvent;
use crate::fixedpoint::FixedScalar;
use crate::tag::TagOnFile;
use crate::transform::{RoutePoint, Vec3};

// ── Byte-order constants (matches cfl.rs) ────────────────────────────────────

const BO_LE: i16 = 0x0001;
// ── ACTF — 44 bytes on disk ───────────────────────────────────────────────────

/// Parsed form of the ACTF header (first 44 bytes of an ACTR chunk's data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorOnFile {
    /// Byte order as stored (BO_LE or BO_BE).
    pub bo: i16,
    /// OS kind as stored.
    pub osk: i16,
    /// Full-route translation vector (world-space offset applied to the whole path).
    pub dxyz_full_rte: Vec3,
    /// Actor runtime ID (unique per-movie; 0 = unused slot).
    pub arid: i32,
    /// First frame the actor is present on.
    pub nfrm_first: i32,
    /// Last frame the actor is present on.
    pub nfrm_last: i32,
    /// Template TAG — which 3DMM content library object this actor uses.
    pub tag_tmpl: TagOnFile,
}

impl ActorOnFile {
    pub const SIZE: usize = 44;
    /// Older pre-release files omit `nfrm_last` — 40 bytes.
    pub const SIZE_OLD: usize = 40;

    /// Parse from exactly 44 bytes (LE or BE auto-detected via `bo` field).
    pub fn from_bytes(b: &[u8; 44]) -> EngineResult<Self> {
        let bo_raw = i16::from_le_bytes(b[0..2].try_into().unwrap());
        let osk_raw = i16::from_le_bytes(b[2..4].try_into().unwrap());

        // Detect byte swap needed (BE file on LE host).
        let swap = !matches!(bo_raw, BO_LE);

        // Validate bo — after swapping if needed, must be BO_LE.
        let bo = if swap { bo_raw.swap_bytes() } else { bo_raw };
        if bo != BO_LE {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }

        let read_i32 = |off: usize| -> i32 {
            let raw = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
            if swap {
                raw.swap_bytes()
            } else {
                raw
            }
        };
        let read_u32 = |off: usize| -> u32 {
            let raw = u32::from_le_bytes(b[off..off + 4].try_into().unwrap());
            if swap {
                raw.swap_bytes()
            } else {
                raw
            }
        };

        let dxyz_full_rte = Vec3 {
            x: FixedScalar(read_i32(4)),
            y: FixedScalar(read_i32(8)),
            z: FixedScalar(read_i32(12)),
        };

        let arid = read_i32(16);
        let nfrm_first = read_i32(20);
        let nfrm_last = read_i32(24);

        // TagOnFile at offset 28 (16 bytes, up through byte 43).
        let tag_sid = read_i32(28);
        // b[32..36] = _pcrf padding (ignored)
        let tag_ctg = read_u32(36);
        let tag_cno = read_u32(40);
        let tag_tmpl = TagOnFile {
            sid: tag_sid,
            ctg: tag_ctg,
            cno: tag_cno,
        };

        Ok(Self {
            bo: bo_raw,
            osk: osk_raw,
            dxyz_full_rte,
            arid,
            nfrm_first,
            nfrm_last,
            tag_tmpl,
        })
    }

    /// Serialize to 44 bytes (LE).
    pub fn to_le_bytes(&self) -> [u8; 44] {
        let mut b = [0u8; 44];
        b[0..2].copy_from_slice(&self.bo.to_le_bytes());
        b[2..4].copy_from_slice(&self.osk.to_le_bytes());
        b[4..8].copy_from_slice(&self.dxyz_full_rte.x.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.dxyz_full_rte.y.0.to_le_bytes());
        b[12..16].copy_from_slice(&self.dxyz_full_rte.z.0.to_le_bytes());
        b[16..20].copy_from_slice(&self.arid.to_le_bytes());
        b[20..24].copy_from_slice(&self.nfrm_first.to_le_bytes());
        b[24..28].copy_from_slice(&self.nfrm_last.to_le_bytes());
        // TagOnFile: sid @ 28, _pcrf @ 32 (zeros), ctg @ 36, cno @ 40
        b[28..32].copy_from_slice(&self.tag_tmpl.sid.to_le_bytes());
        // b[32..36] already 0 (_pcrf)
        b[36..40].copy_from_slice(&self.tag_tmpl.ctg.to_le_bytes());
        b[40..44].copy_from_slice(&self.tag_tmpl.cno.to_le_bytes());
        b
    }
}

// ── Runtime Actor ────────────────────────────────────────────────────────────

/// Runtime actor — fully deserialized from ACTR + PATH + GGAE chunks.
#[derive(Debug, Clone)]
pub struct Actor {
    /// Unique actor runtime ID.
    pub arid: i32,
    /// World-space path translation.
    pub dxyz_full_rte: Vec3,
    /// First frame this actor appears on.
    pub nfrm_first: i32,
    /// Last frame this actor appears on.
    pub nfrm_last: i32,
    /// Content library template reference.
    pub tag_tmpl: TagOnFile,
    /// Route path points (from PATH GL sub-chunk).
    pub route: Vec<RoutePoint>,
    /// Events (from GGAE GG sub-chunk).
    pub events: Vec<ActorEvent>,
}

impl Actor {
    /// Construct from parsed ACTF + decoded sub-chunks.
    pub fn new(header: ActorOnFile, route: Vec<RoutePoint>, events: Vec<ActorEvent>) -> Self {
        Self {
            arid: header.arid,
            dxyz_full_rte: header.dxyz_full_rte,
            nfrm_first: header.nfrm_first,
            nfrm_last: header.nfrm_last,
            tag_tmpl: header.tag_tmpl,
            route,
            events,
        }
    }

    /// Total number of frames this actor spans.
    pub fn frame_count(&self) -> i32 {
        (self.nfrm_last - self.nfrm_first).max(0)
    }

    /// Events that fire on a specific frame.
    pub fn events_at_frame(&self, nfrm: i32) -> impl Iterator<Item = &ActorEvent> {
        self.events.iter().filter(move |e| e.header.nfrm == nfrm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::CTG_ACTR;

    fn make_actf_bytes(arid: i32, nfrm_first: i32, nfrm_last: i32) -> [u8; 44] {
        let mut b = [0u8; 44];
        // bo = LE
        b[0..2].copy_from_slice(&(BO_LE as i16).to_le_bytes());
        // osk = Windows (0x7769 = 'wi')
        b[2..4].copy_from_slice(&(0x7769i16).to_le_bytes());
        // dxyz_full_rte = (1.0, 2.0, 3.0)
        b[4..8].copy_from_slice(&0x0001_0000i32.to_le_bytes());
        b[8..12].copy_from_slice(&0x0002_0000i32.to_le_bytes());
        b[12..16].copy_from_slice(&0x0003_0000i32.to_le_bytes());
        // arid, nfrm_first, nfrm_last
        b[16..20].copy_from_slice(&arid.to_le_bytes());
        b[20..24].copy_from_slice(&nfrm_first.to_le_bytes());
        b[24..28].copy_from_slice(&nfrm_last.to_le_bytes());
        // tag_tmpl: sid=0, _pcrf=0, ctg=ACTR, cno=1
        b[28..32].copy_from_slice(&0i32.to_le_bytes());
        // b[32..36] = 0 (_pcrf)
        b[36..40].copy_from_slice(&CTG_ACTR.to_le_bytes());
        b[40..44].copy_from_slice(&1u32.to_le_bytes());
        b
    }

    #[test]
    fn test_actf_size() {
        assert_eq!(ActorOnFile::SIZE, 44);
    }

    #[test]
    fn test_actf_parse() {
        let bytes = make_actf_bytes(42, 0, 30);
        let actf = ActorOnFile::from_bytes(&bytes).unwrap();
        assert_eq!(actf.arid, 42);
        assert_eq!(actf.nfrm_first, 0);
        assert_eq!(actf.nfrm_last, 30);
        assert_eq!(actf.dxyz_full_rte.x.to_f64(), 1.0);
        assert_eq!(actf.dxyz_full_rte.y.to_f64(), 2.0);
        assert_eq!(actf.dxyz_full_rte.z.to_f64(), 3.0);
        assert_eq!(actf.tag_tmpl.ctg, CTG_ACTR);
        assert_eq!(actf.tag_tmpl.cno, 1);
    }

    #[test]
    fn test_actf_roundtrip() {
        let bytes = make_actf_bytes(7, 5, 25);
        let actf = ActorOnFile::from_bytes(&bytes).unwrap();
        let roundtrip = actf.to_le_bytes();
        assert_eq!(bytes, roundtrip);
    }

    #[test]
    fn test_actor_frame_count() {
        let actf = ActorOnFile::from_bytes(&make_actf_bytes(1, 10, 40)).unwrap();
        let actor = Actor::new(actf, vec![], vec![]);
        assert_eq!(actor.frame_count(), 30);
    }

    #[test]
    fn test_actor_events_at_frame() {
        use crate::events::{aet, ActorEvent, AevHeader, EventPayload};
        use crate::fixedpoint::FixedScalar;
        use crate::transform::RouteLocation;

        let rtel = RouteLocation {
            irpt: 0,
            dwr: FixedScalar::ZERO,
            dnwr: FixedScalar::ZERO,
        };
        let evt = ActorEvent {
            header: AevHeader {
                aet: aet::HIDE,
                nfrm: 5,
                rtel,
            },
            payload: EventPayload::Hide { visible: false },
        };
        let actf = ActorOnFile::from_bytes(&make_actf_bytes(1, 0, 10)).unwrap();
        let actor = Actor::new(actf, vec![], vec![evt]);

        let at5: Vec<_> = actor.events_at_frame(5).collect();
        assert_eq!(at5.len(), 1);
        let at6: Vec<_> = actor.events_at_frame(6).collect();
        assert!(at6.is_empty());
    }
}
