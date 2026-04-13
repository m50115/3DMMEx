//! Actor event types (AEV — Actor EVent).
//!
//! Each event entry in the GGAE chunk has:
//!   - Fixed part (20 bytes): [i32 aet][i32 nfrm][RouteLocation:12]
//!   - Variable part: event-type-specific payload bytes
//!
//! Event type codes (aet) are from actor.h `AEV_*` constants.

use crate::transform::RouteLocation;
use crate::error::{EngineError, EngineResult};
use crate::tag::TagOnFile;
use crate::fixedpoint::{FixedScalar, FixedAngle};

// ── AEV type codes ───────────────────────────────────────────────────────────

/// Actor event type constants from actor.h.
#[allow(dead_code)]
pub mod aet {
    pub const HIDE: i32       = 0x0001; // show/hide toggle
    pub const FREEZE: i32     = 0x0002; // freeze/unfreeze
    pub const ORIENT: i32     = 0x0004; // orientation change
    pub const ROTATE: i32     = 0x0008; // rotation
    pub const SCALE: i32      = 0x0010; // scale change
    pub const ACTOR: i32      = 0x0020; // sub-actor embed (TAG reference)
    pub const COST: i32       = 0x0040; // costume change (TAG reference)
    pub const SOUND: i32      = 0x0080; // sound trigger (TAG reference)
    pub const SPEECH: i32     = 0x0100; // speech bubble (TAG reference)
    pub const SIZE_POS: i32   = 0x0200; // size + position
    pub const PULL: i32       = 0x0400; // pull-through (path attachment)
    pub const STEP: i32       = 0x0800; // step animation
    pub const RTEL: i32       = 0x1000; // route location reset
}

// ── Fixed AEV header — 20 bytes ──────────────────────────────────────────────

/// Fixed header common to all AEV entries (20 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AevHeader {
    /// Event type flags (aet::* constants).
    pub aet: i32,
    /// Frame number this event fires on.
    pub nfrm: i32,
    /// Route location at the time of the event.
    pub rtel: RouteLocation,
}

impl AevHeader {
    pub const SIZE: usize = 20;

    pub fn from_le_bytes(b: &[u8; 20]) -> Self {
        Self {
            aet:  i32::from_le_bytes(b[0..4].try_into().unwrap()),
            nfrm: i32::from_le_bytes(b[4..8].try_into().unwrap()),
            rtel: RouteLocation::from_le_bytes(b[8..20].try_into().unwrap()),
        }
    }

    pub fn to_le_bytes(&self) -> [u8; 20] {
        let mut b = [0u8; 20];
        b[0..4].copy_from_slice(&self.aet.to_le_bytes());
        b[4..8].copy_from_slice(&self.nfrm.to_le_bytes());
        b[8..20].copy_from_slice(&self.rtel.to_le_bytes());
        b
    }
}

// ── Per-type variable payloads ───────────────────────────────────────────────

/// Orientation payload: 3 BRA angles (6 bytes total, padded to 8 with 2 zero bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrientPayload {
    pub xa: FixedAngle, // pitch
    pub ya: FixedAngle, // yaw
    pub za: FixedAngle, // roll
}

impl OrientPayload {
    pub const SIZE: usize = 8; // 3×u16 + 2 pad

    pub fn from_le_bytes(b: &[u8; 8]) -> Self {
        Self {
            xa: FixedAngle(u16::from_le_bytes(b[0..2].try_into().unwrap())),
            ya: FixedAngle(u16::from_le_bytes(b[2..4].try_into().unwrap())),
            za: FixedAngle(u16::from_le_bytes(b[4..6].try_into().unwrap())),
        }
        // b[6..8] = padding (ignored)
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..2].copy_from_slice(&self.xa.0.to_le_bytes());
        b[2..4].copy_from_slice(&self.ya.0.to_le_bytes());
        b[4..6].copy_from_slice(&self.za.0.to_le_bytes());
        b
    }
}

/// Scale payload: 3 BRS scalars (12 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalePayload {
    pub sx: FixedScalar,
    pub sy: FixedScalar,
    pub sz: FixedScalar,
}

impl ScalePayload {
    pub const SIZE: usize = 12;

    pub fn from_le_bytes(b: &[u8; 12]) -> Self {
        Self {
            sx: FixedScalar(i32::from_le_bytes(b[0..4].try_into().unwrap())),
            sy: FixedScalar(i32::from_le_bytes(b[4..8].try_into().unwrap())),
            sz: FixedScalar(i32::from_le_bytes(b[8..12].try_into().unwrap())),
        }
    }

    pub fn to_le_bytes(self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0..4].copy_from_slice(&self.sx.0.to_le_bytes());
        b[4..8].copy_from_slice(&self.sy.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.sz.0.to_le_bytes());
        b
    }
}

/// Tag-reference payload (ACTOR / COST / SOUND / SPEECH): 16-byte TagOnFile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPayload(pub TagOnFile);

impl TagPayload {
    pub const SIZE: usize = TagOnFile::SIZE; // 16

    pub fn from_le_bytes(b: &[u8; 16]) -> Self {
        Self(TagOnFile::from_le_bytes(b))
    }

    pub fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }
}

/// Step payload: i32 cel index (4 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepPayload {
    pub icel: i32,
}

impl StepPayload {
    pub const SIZE: usize = 4;

    pub fn from_le_bytes(b: &[u8; 4]) -> Self {
        Self { icel: i32::from_le_bytes(*b) }
    }

    pub fn to_le_bytes(self) -> [u8; 4] {
        self.icel.to_le_bytes()
    }
}

// ── ActorEvent — unified event type ─────────────────────────────────────────

/// A fully parsed actor event entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorEvent {
    pub header: AevHeader,
    pub payload: EventPayload,
}

/// Discriminated union of per-type variable payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    /// AEV_HIDE — show/hide (no variable data).
    Hide { visible: bool },
    /// AEV_FREEZE — freeze/unfreeze (no variable data).
    Freeze { frozen: bool },
    /// AEV_ORIENT — 3 BRA angles.
    Orient(OrientPayload),
    /// AEV_ROTATE — 3 BRA angles (same layout as Orient).
    Rotate(OrientPayload),
    /// AEV_SCALE — 3 BRS scalars.
    Scale(ScalePayload),
    /// AEV_ACTOR — embedded actor TAG.
    Actor(TagPayload),
    /// AEV_COST — costume TAG.
    Cost(TagPayload),
    /// AEV_SOUND — sound TAG.
    Sound(TagPayload),
    /// AEV_SPEECH — speech TAG.
    Speech(TagPayload),
    /// AEV_SIZE_POS — no extra variable data (position comes from rtel).
    SizePos,
    /// AEV_PULL — pull-through (no variable data).
    Pull,
    /// AEV_STEP — cel index.
    Step(StepPayload),
    /// AEV_RTEL — route location reset (no extra variable data — rtel in header suffices).
    Rtel,
    /// Unknown event type; raw variable bytes preserved.
    Unknown { aet: i32, var_data: Vec<u8> },
}

impl ActorEvent {
    /// Parse a complete event from fixed bytes + variable bytes.
    pub fn parse(fixed: &[u8; 20], var: &[u8]) -> EngineResult<Self> {
        let header = AevHeader::from_le_bytes(fixed);
        let payload = EventPayload::parse(header.aet, var)?;
        Ok(Self { header, payload })
    }
}

impl EventPayload {
    pub fn parse(aet: i32, var: &[u8]) -> EngineResult<Self> {
        match aet {
            self::aet::HIDE    => Ok(EventPayload::Hide    { visible: var.first().copied().unwrap_or(1) != 0 }),
            self::aet::FREEZE  => Ok(EventPayload::Freeze  { frozen: var.first().copied().unwrap_or(0) != 0 }),
            self::aet::ORIENT  => parse_orient_payload(var).map(EventPayload::Orient),
            self::aet::ROTATE  => parse_orient_payload(var).map(EventPayload::Rotate),
            self::aet::SCALE   => {
                if var.len() < ScalePayload::SIZE {
                    return Err(EngineError::UnexpectedEof {
                        what: "AEV_SCALE payload",
                        need: ScalePayload::SIZE,
                        got: var.len(),
                    });
                }
                Ok(EventPayload::Scale(ScalePayload::from_le_bytes(
                    var[..12].try_into().unwrap(),
                )))
            }
            self::aet::ACTOR   => parse_tag_payload(var, "AEV_ACTOR").map(EventPayload::Actor),
            self::aet::COST    => parse_tag_payload(var, "AEV_COST").map(EventPayload::Cost),
            self::aet::SOUND   => parse_tag_payload(var, "AEV_SOUND").map(EventPayload::Sound),
            self::aet::SPEECH  => parse_tag_payload(var, "AEV_SPEECH").map(EventPayload::Speech),
            self::aet::SIZE_POS => Ok(EventPayload::SizePos),
            self::aet::PULL    => Ok(EventPayload::Pull),
            self::aet::STEP    => {
                if var.len() < StepPayload::SIZE {
                    return Err(EngineError::UnexpectedEof {
                        what: "AEV_STEP payload",
                        need: StepPayload::SIZE,
                        got: var.len(),
                    });
                }
                Ok(EventPayload::Step(StepPayload::from_le_bytes(
                    var[..4].try_into().unwrap(),
                )))
            }
            self::aet::RTEL    => Ok(EventPayload::Rtel),
            other => Ok(EventPayload::Unknown { aet: other, var_data: var.to_vec() }),
        }
    }

    /// Serialize the variable-length payload to bytes.
    pub fn to_var_bytes(&self) -> Vec<u8> {
        match self {
            EventPayload::Hide { visible } => vec![*visible as u8],
            EventPayload::Freeze { frozen } => vec![*frozen as u8],
            EventPayload::Orient(p) | EventPayload::Rotate(p) => p.to_le_bytes().to_vec(),
            EventPayload::Scale(p)   => p.to_le_bytes().to_vec(),
            EventPayload::Actor(p)
            | EventPayload::Cost(p)
            | EventPayload::Sound(p)
            | EventPayload::Speech(p) => p.to_le_bytes().to_vec(),
            EventPayload::Step(p)    => p.to_le_bytes().to_vec(),
            EventPayload::SizePos | EventPayload::Pull | EventPayload::Rtel => vec![],
            EventPayload::Unknown { var_data, .. } => var_data.clone(),
        }
    }
}

fn parse_orient_payload(var: &[u8]) -> EngineResult<OrientPayload> {
    if var.len() < OrientPayload::SIZE {
        return Err(EngineError::UnexpectedEof {
            what: "AEV_ORIENT/ROTATE payload",
            need: OrientPayload::SIZE,
            got: var.len(),
        });
    }
    Ok(OrientPayload::from_le_bytes(var[..8].try_into().unwrap()))
}

fn parse_tag_payload(var: &[u8], what: &'static str) -> EngineResult<TagPayload> {
    if var.len() < TagPayload::SIZE {
        return Err(EngineError::UnexpectedEof {
            what,
            need: TagPayload::SIZE,
            got: var.len(),
        });
    }
    Ok(TagPayload::from_le_bytes(var[..16].try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixedpoint::FixedScalar;

    fn dummy_header(aet: i32) -> [u8; 20] {
        let mut b = [0u8; 20];
        b[0..4].copy_from_slice(&aet.to_le_bytes());
        // nfrm = 5
        b[4..8].copy_from_slice(&5i32.to_le_bytes());
        // rtel all zeros
        b
    }

    #[test]
    fn test_aev_header_size() {
        assert_eq!(AevHeader::SIZE, 20);
    }

    #[test]
    fn test_orient_roundtrip() {
        let p = OrientPayload {
            xa: FixedAngle(1000),
            ya: FixedAngle(2000),
            za: FixedAngle(3000),
        };
        let bytes = p.to_le_bytes();
        let p2 = OrientPayload::from_le_bytes(&bytes);
        assert_eq!(p, p2);
    }

    #[test]
    fn test_scale_roundtrip() {
        let p = ScalePayload {
            sx: FixedScalar(0x0001_0000),
            sy: FixedScalar(0x0002_0000),
            sz: FixedScalar(0x0000_8000),
        };
        let bytes = p.to_le_bytes();
        let p2 = ScalePayload::from_le_bytes(&bytes);
        assert_eq!(p, p2);
    }

    #[test]
    fn test_parse_hide_event() {
        let fixed = dummy_header(aet::HIDE);
        let var = vec![1u8]; // visible = true
        let evt = ActorEvent::parse(&fixed, &var).unwrap();
        assert_eq!(evt.payload, EventPayload::Hide { visible: true });
        assert_eq!(evt.header.nfrm, 5);
    }

    #[test]
    fn test_parse_orient_event() {
        let mut var = [0u8; 8];
        // xa = 90 degrees in BRA
        let xa = FixedAngle::from_degrees(90.0);
        var[0..2].copy_from_slice(&xa.0.to_le_bytes());
        let fixed = dummy_header(aet::ORIENT);
        let evt = ActorEvent::parse(&fixed, &var).unwrap();
        if let EventPayload::Orient(p) = evt.payload {
            assert!((p.xa.to_degrees() - 90.0).abs() < 0.1);
        } else {
            panic!("wrong payload type");
        }
    }

    #[test]
    fn test_parse_unknown_event() {
        let fixed = dummy_header(0xDEAD);
        let var = vec![0xAB, 0xCD];
        let evt = ActorEvent::parse(&fixed, &var).unwrap();
        if let EventPayload::Unknown { aet, var_data } = evt.payload {
            assert_eq!(aet, 0xDEAD);
            assert_eq!(var_data, vec![0xAB, 0xCD]);
        } else {
            panic!("expected Unknown");
        }
    }

    #[test]
    fn test_var_bytes_roundtrip_orient() {
        let p = OrientPayload {
            xa: FixedAngle(500),
            ya: FixedAngle(1500),
            za: FixedAngle(2500),
        };
        let payload = EventPayload::Orient(p);
        let var_bytes = payload.to_var_bytes();
        let parsed = EventPayload::parse(aet::ORIENT, &var_bytes).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn test_rtel_no_var_data() {
        let fixed = dummy_header(aet::RTEL);
        let evt = ActorEvent::parse(&fixed, &[]).unwrap();
        assert_eq!(evt.payload, EventPayload::Rtel);
        let bytes = evt.payload.to_var_bytes();
        assert!(bytes.is_empty());
    }
}
