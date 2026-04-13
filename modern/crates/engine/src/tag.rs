//! Chunk type tags (CTG) and resource tag types.
//!
//! CTG tags are stored as u32 with the first ASCII character in the most
//! significant byte (C multi-char literal convention). Use `u32::from_be_bytes`
//! for constants and `to_be_bytes()` to display them.

use std::fmt;

// ── Movie / Scene / Actor chunk types ──────────────────────────────────────

/// Root movie chunk.
pub const CTG_MVIE: u32 = u32::from_be_bytes(*b"MVIE");
/// Scene chunk.
pub const CTG_SCEN: u32 = u32::from_be_bytes(*b"SCEN");
/// Actor chunk.
pub const CTG_ACTR: u32 = u32::from_be_bytes(*b"ACTR");
/// Text-box chunk.
pub const CTG_TBOX: u32 = u32::from_be_bytes(*b"TBOX");
/// Sound chunk.
pub const CTG_MSND: u32 = u32::from_be_bytes(*b"MSND");
/// Roll-call (GST) chunk — stored under MVIE with kchidGstMactr.
pub const CTG_GST: u32 = u32::from_be_bytes(*b"GST\0");

// ── BRender asset chunk types ───────────────────────────────────────────────

/// BRender model data chunk (on-disk tag for mesh geometry).
pub const CTG_BMDL: u32 = u32::from_be_bytes(*b"BMDL");
/// MODL wrapper class tag (used by C++ BACO system, rarely on disk).
pub const CTG_MODL: u32 = u32::from_be_bytes(*b"MODL");
/// Template (actor definition: body, costume, action references).
pub const CTG_TMPL: u32 = u32::from_be_bytes(*b"TMPL");
/// Background (camera, lights, positions).
pub const CTG_BKGD: u32 = u32::from_be_bytes(*b"BKGD");
/// BRender material chunk tag (on-disk, in mtrls.3cn / .3th files).
/// Content files use 'MTRL', not 'BMTL' — BMTL is the C++ class name.
pub const CTG_MTRL: u32 = u32::from_be_bytes(*b"MTRL");
/// Costume material (CMTL) — references MTRL with costume metadata.
pub const CTG_CMTL: u32 = u32::from_be_bytes(*b"CMTL");
/// Action definition chunk.
pub const CTG_ACTN: u32 = u32::from_be_bytes(*b"ACTN");

// ── Actor sub-chunks ────────────────────────────────────────────────────────

/// GL of route points for an actor.
pub const CTG_PATH: u32 = u32::from_be_bytes(*b"PATH");
/// GG of actor events.
pub const CTG_GGAE: u32 = u32::from_be_bytes(*b"GGAE");

// ── Well-known child IDs ────────────────────────────────────────────────────

/// chid for the ACTR chunk inside a SCEN.
pub const CHID_ACTR_0: u32 = 0;
/// chid for GST roll-call under MVIE.
pub const CHID_GST_MACTR: u32 = 0x8000_0000;

// ── Sid (source/content type) values ───────────────────────────────────────

/// Tag points to a chunk in the same file (sid = 0).
pub const SID_CURRENT_FILE: i32 = 0;
/// Tag points to an external content file.
pub const SID_EXTERNAL: i32 = -1;

// ── TagOnFile (TAGF) — 16 bytes on-disk ─────────────────────────────────────
//
// Layout (always LE on disk inside the GG variable buffer):
//   [i32 sid][i32 _pcrf_pad][u32 ctg][u32 cno]
//
// `_pcrf_pad` is a C pointer field that is always written as 0.

/// Serialized form of a TAG reference as it appears in chunk data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagOnFile {
    pub sid: i32,
    pub ctg: u32,
    pub cno: u32,
}

impl TagOnFile {
    /// Size in bytes on disk.
    pub const SIZE: usize = 16;

    /// Parse from a 16-byte slice (LE, with 4-byte `_pcrf` padding at offset 4).
    pub fn from_le_bytes(b: &[u8; 16]) -> Self {
        let sid = i32::from_le_bytes(b[0..4].try_into().unwrap());
        // b[4..8] = _pcrf padding (ignored)
        let ctg = u32::from_le_bytes(b[8..12].try_into().unwrap());
        let cno = u32::from_le_bytes(b[12..16].try_into().unwrap());
        Self { sid, ctg, cno }
    }

    /// Serialize to 16 bytes (LE, `_pcrf` written as zero).
    pub fn to_le_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&self.sid.to_le_bytes());
        // b[4..8] stays zero (_pcrf)
        b[8..12].copy_from_slice(&self.ctg.to_le_bytes());
        b[12..16].copy_from_slice(&self.cno.to_le_bytes());
        b
    }

    /// Return true if this tag is a null/empty reference.
    pub fn is_null(&self) -> bool {
        self.ctg == 0 && self.cno == 0
    }

    /// 4-char display string for `ctg` field.
    pub fn ctg_str(&self) -> String {
        self.ctg
            .to_be_bytes()
            .iter()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
            .collect()
    }
}

impl fmt::Display for TagOnFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TAG(sid={} {}/{:#010x})", self.sid, self.ctg_str(), self.cno)
    }
}

// ── Runtime Tag ─────────────────────────────────────────────────────────────

/// Runtime tag reference (not padded — 12 bytes if ever serialized inline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub sid: i32,
    pub ctg: u32,
    pub cno: u32,
}

impl From<TagOnFile> for Tag {
    fn from(t: TagOnFile) -> Self {
        Self { sid: t.sid, ctg: t.ctg, cno: t.cno }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ctg_str: String = self
            .ctg
            .to_be_bytes()
            .iter()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
            .collect();
        write!(f, "TAG(sid={} {}/{:#010x})", self.sid, ctg_str, self.cno)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctg_constants_display() {
        // Verify CTG constants display correctly via to_be_bytes
        let mvie: String = CTG_MVIE.to_be_bytes().iter().map(|&b| b as char).collect();
        assert_eq!(mvie, "MVIE");

        let scen: String = CTG_SCEN.to_be_bytes().iter().map(|&b| b as char).collect();
        assert_eq!(scen, "SCEN");

        let actr: String = CTG_ACTR.to_be_bytes().iter().map(|&b| b as char).collect();
        assert_eq!(actr, "ACTR");
    }

    #[test]
    fn test_tag_on_file_roundtrip() {
        let tag = TagOnFile { sid: 0, ctg: CTG_MVIE, cno: 0x1234 };
        let bytes = tag.to_le_bytes();
        assert_eq!(bytes.len(), 16);

        // sid at [0..4]
        assert_eq!(i32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0i32);
        // _pcrf at [4..8] must be zero
        assert_eq!(&bytes[4..8], &[0u8; 4]);
        // ctg at [8..12]
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), CTG_MVIE);
        // cno at [12..16]
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 0x1234u32);

        let parsed = TagOnFile::from_le_bytes(&bytes);
        assert_eq!(parsed, tag);
    }

    #[test]
    fn test_tag_on_file_null() {
        let null = TagOnFile { sid: 0, ctg: 0, cno: 0 };
        assert!(null.is_null());

        let non_null = TagOnFile { sid: 0, ctg: CTG_ACTR, cno: 1 };
        assert!(!non_null.is_null());
    }

    #[test]
    fn test_tag_from_tag_on_file() {
        let tof = TagOnFile { sid: -1, ctg: CTG_SCEN, cno: 42 };
        let tag: Tag = tof.into();
        assert_eq!(tag.sid, -1);
        assert_eq!(tag.ctg, CTG_SCEN);
        assert_eq!(tag.cno, 42);
    }
}
