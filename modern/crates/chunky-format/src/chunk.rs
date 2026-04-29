//! Core chunk structures: ChunkId (CKI), ChunkEntry (CRP), ChildRef (KID), ChunkFlags.

use bitflags::bitflags;

/// CKI — Chunk Identifier (8 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId {
    /// Chunk type tag (CTG) — typically 4 ASCII chars packed as u32
    pub ctg: u32,
    /// Chunk number (CNO)
    pub cno: u32,
}

impl ChunkId {
    pub const SIZE: usize = 8;
    /// BOM: two i32 fields = 11 11 00 00 = 0xF0000000
    pub const BOM: u32 = 0xF000_0000;
}

impl std::fmt::Display for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // CTG is stored as LE u32 in the file (first char at highest byte).
        // Print bytes in BE order to get the natural 4-char ASCII tag.
        let bytes = self.ctg.to_be_bytes();
        let tag_str: String = bytes
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '?'
                }
            })
            .collect();
        write!(f, "'{}':{}", tag_str, self.cno)
    }
}

bitflags! {
    /// CRP flags (grfcrp).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChunkFlags: u32 {
        const NONE       = 0x00;
        const ON_EXTRA   = 0x01;
        const LONER      = 0x02;
        const PACKED     = 0x04;
        const MARK_T     = 0x08;
        const FOREST     = 0x10;
    }
}

/// KID — Child reference (12 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildRef {
    /// Child chunk identifier
    pub id: ChunkId,
    /// Child relationship ID (CHID)
    pub chid: u32,
}

impl ChildRef {
    pub const SIZE: usize = 12;
    /// BOM: CKI(i32,i32) + i32 = 11 11 11 00 = 0xFC000000
    pub const BOM: u32 = 0xFC00_0000;
}

/// CRP — Chunk entry in the file index.
///
/// Represents one chunk's metadata. Supports both CRPSM (20-byte small)
/// and CRPBG (32-byte big) on-disk formats.
#[derive(Debug, Clone)]
pub struct ChunkEntry {
    /// Chunk identifier (type + number)
    pub id: ChunkId,
    /// File position of chunk data
    pub fp: u32,
    /// Data size in bytes
    pub cb: u32,
    /// Flags (compression, forest, etc.)
    pub flags: ChunkFlags,
    /// Number of child chunks
    pub child_count: u32,
    /// Number of parent references
    pub ref_count: u32,
    /// Run-time ID (only in big index format)
    pub rti: u32,
    /// Child references
    pub children: Vec<ChildRef>,
    /// Optional chunk name
    pub name: Option<String>,
}

impl ChunkEntry {
    /// Size of CRPSM (small index entry) on disk
    pub const SIZE_SMALL: usize = 20;
    /// Size of CRPBG (big index entry) on disk
    pub const SIZE_BIG: usize = 32;
    /// BOM for CRPSM
    pub const BOM_SMALL: u32 = 0xFF50_0000;
    /// BOM for CRPBG (grfcrp format)
    pub const BOM_BIG: u32 = 0xFFFF_0000;

    /// Returns true if chunk data is compressed
    pub fn is_packed(&self) -> bool {
        self.flags.contains(ChunkFlags::PACKED)
    }

    /// Returns true if chunk contains embedded forest
    pub fn is_forest(&self) -> bool {
        self.flags.contains(ChunkFlags::FOREST)
    }
}
