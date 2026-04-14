//! CFL — Chunky File reader/writer.
//!
//! # CHN2 File Layout
//!
//! ```text
//! [0]   CFP header        128 bytes  — magic, creator, version, byte-order, fp_index, cb_index, fp_map, cb_map
//! [128] chunk data ...    variable   — raw chunk bytes at their fp offsets (may be KCD2-compressed; flag = PACKED)
//! [fp_index] GGF index   cb_index   — GG in file-index format (GGF, 20-byte header variant)
//! [fp_map]  FSM (opt.)   cb_map     — Free Space Map as GL of 8-byte {fp,cb} entries
//! ```
//!
//! # GGF Index On-Disk Layout
//!
//! The chunk index is stored as a GGF (General Group, file variant).
//! Layout confirmed from Kauai `groups.cpp CbOnFile()`:
//!
//! ```text
//! [GGF header:  20 bytes]   bo(u16) + osk(u16) + ilocMac(u32) + bvMac(u32) + clocFree(u32) + cbFixed(u32)
//! [data buffer: bvMac bytes]  fixed+variable data for ALL elements at LOC[i].bv offsets (GG-allocator-scattered)
//! [LOC table:   ilocMac×8]  {bv:u32, cb:u32} per element — points into data buffer
//! ```
//!
//! CbOnFile = 20 + bvMac + ilocMac*8.
//!
//! Element i: fixed bytes = `data_buf[LOC[i].bv .. LOC[i].bv+cbFixed]`
//!            variable bytes = `data_buf[LOC[i].bv+cbFixed .. LOC[i].bv+LOC[i].cb]`
//!
//! NOTE: This is NOT the same as `collections::GenericGroup` which uses a different 12-byte header
//! layout (fixed-sequential then LOC then variable). GGF is the file-index-specific format.
//!
//! # CRPSM Fixed Entry (20 bytes, version ≥ 4)
//!
//! ```text
//! [ctg:u32][cno:u32][fp:u32][lu:u32][ckid:u16][ccrp_ref:u16]
//! ```
//! lu = (cb << 8) | (flags & 0xFF)  — packs chunk size and flags.
//!
//! # Variable Entry (after fixed, within LOC[i].cb)
//!
//! ```text
//! ckid × 12 bytes  — KID children: [ctg:u32][cno:u32][chid:u32]
//! optional STN name: [osk:u16][cch:u8][chars:cch bytes][null:1]
//! ```
//!
//! # Per-Chunk Compression
//!
//! Flag `PACKED` (0x04) in chunk flags → data is KCD2-compressed.
//! Each chunk independently compressed. Writing uncompressed is valid (3DMM reads both).
//! fp_index, fp_map, fp_mac are byte offsets from file start.

use std::io::{Read, Seek, SeekFrom, Write};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::bom::{needs_swap, swap_bytes_bom};
use crate::chunk::{ChunkId, ChunkEntry, ChunkFlags, ChildRef};
use crate::codec;
use crate::error::{ChunkyError, Result};

/// Magic number: "CHN2" bytes read as LE u32 (LE / Windows files).
/// In the file the bytes are literally 0x43 0x48 0x4E 0x32 ("CHN2").
/// `u32::from_le_bytes([0x43, 0x48, 0x4E, 0x32])` = 0x324E_4843.
pub const MAGIC_CHUNKY: u32 = 0x324E_4843;
/// Magic for BE (Mac) chunky files: bytes in file are reversed ("2NHC"),
/// which when read as LE u32 = 0x4348_4E32.
pub const MAGIC_CHUNKY_BE: u32 = 0x4348_4E32;

/// CFP header size
pub const HEADER_SIZE: usize = 128;

/// BOM for CFP header
pub const BOM_CFP: u32 = 0xB55F_FC00;

/// Chunky file version constants
pub const VERSION_CURRENT: u16 = 5;
pub const VERSION_BACK: u16 = 4;
pub const VERSION_MIN: u16 = 1;
pub const VERSION_MIN_STN_NAMES: u16 = 3;
pub const VERSION_MIN_SMALL_INDEX: u16 = 4;
pub const VERSION_MIN_FOREST: u16 = 5;

/// 3D Movie Maker creator tag: 'SOC ' (Socrates)
pub const CTG_CREATOR_3DMM: u32 = u32::from_le_bytes([b'S', b'O', b'C', b' ']);

/// Chunky File Prefix (CFP) — 128 byte header.
#[derive(Debug, Clone)]
pub struct ChunkyHeader {
    pub magic: u32,
    pub ctg_creator: u32,
    pub version_current: u16,
    pub version_back: u16,
    pub byte_order: u16,
    pub os_kind: u16,
    pub fp_mac: u32,
    pub fp_index: u32,
    pub cb_index: u32,
    pub fp_map: u32,
    pub cb_map: u32,
    pub reserved: [u32; 23],
}

impl ChunkyHeader {
    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        let mut buf = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buf)?;

        // Check magic — determine endianness
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let is_be = match magic {
            MAGIC_CHUNKY => false,
            MAGIC_CHUNKY_BE => true,
            _ => return Err(ChunkyError::InvalidMagic {
                expected: MAGIC_CHUNKY,
                actual: magic,
            }),
        };

        // If big-endian, swap all fields according to BOM
        if is_be {
            swap_bytes_bom(&mut buf[..36], BOM_CFP);
            // Also swap the reserved u32 array
            for i in 0..23 {
                let off = 36 + i * 4;
                buf.swap(off, off + 3);
                buf.swap(off + 1, off + 2);
            }
        }

        let mut c = std::io::Cursor::new(&buf[..]);

        let header = ChunkyHeader {
            magic: c.read_u32::<LittleEndian>()?,
            ctg_creator: c.read_u32::<LittleEndian>()?,
            version_current: c.read_u16::<LittleEndian>()?,
            version_back: c.read_u16::<LittleEndian>()?,
            byte_order: c.read_u16::<LittleEndian>()?,
            os_kind: c.read_u16::<LittleEndian>()?,
            fp_mac: c.read_u32::<LittleEndian>()?,
            fp_index: c.read_u32::<LittleEndian>()?,
            cb_index: c.read_u32::<LittleEndian>()?,
            fp_map: c.read_u32::<LittleEndian>()?,
            cb_map: c.read_u32::<LittleEndian>()?,
            reserved: {
                let mut r = [0u32; 23];
                for item in &mut r {
                    *item = c.read_u32::<LittleEndian>()?;
                }
                r
            },
        };

        // Validate version
        if header.version_back > VERSION_CURRENT {
            return Err(ChunkyError::UnsupportedVersion {
                version: header.version_back,
                min: VERSION_MIN,
            });
        }

        Ok(header)
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LittleEndian>(self.magic)?;
        writer.write_u32::<LittleEndian>(self.ctg_creator)?;
        writer.write_u16::<LittleEndian>(self.version_current)?;
        writer.write_u16::<LittleEndian>(self.version_back)?;
        writer.write_u16::<LittleEndian>(self.byte_order)?;
        writer.write_u16::<LittleEndian>(self.os_kind)?;
        writer.write_u32::<LittleEndian>(self.fp_mac)?;
        writer.write_u32::<LittleEndian>(self.fp_index)?;
        writer.write_u32::<LittleEndian>(self.cb_index)?;
        writer.write_u32::<LittleEndian>(self.fp_map)?;
        writer.write_u32::<LittleEndian>(self.cb_map)?;
        for &r in &self.reserved {
            writer.write_u32::<LittleEndian>(r)?;
        }
        Ok(())
    }

    /// Whether this file uses the small (20-byte) index format
    pub fn uses_small_index(&self) -> bool {
        self.version_current >= VERSION_MIN_SMALL_INDEX
    }
}

/// Represents a complete chunky file in memory.
#[derive(Debug)]
pub struct ChunkyFile {
    /// File header
    pub header: ChunkyHeader,
    /// All chunk entries (the index)
    pub chunks: Vec<ChunkEntry>,
    /// Raw chunk data keyed by (ctg, cno). Stored as-is from disk (may be compressed).
    pub chunk_data: std::collections::HashMap<(u32, u32), Vec<u8>>,
    /// Free space map entries
    pub free_map: Vec<FreeSpaceEntry>,
    /// Raw index bytes (for bit-perfect round-trip)
    pub raw_index: Vec<u8>,
}

/// FSM — Free Space Map entry (8 bytes).
#[derive(Debug, Clone, Copy)]
pub struct FreeSpaceEntry {
    pub fp: u32,
    pub cb: u32,
}

impl FreeSpaceEntry {
    pub const SIZE: usize = 8;
    pub const BOM: u32 = 0xF000_0000;
}

impl ChunkyFile {
    /// Read a chunky file from a reader.
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let header = ChunkyHeader::read(reader)?;
        let swap = needs_swap(header.byte_order);

        // Read raw index
        reader.seek(SeekFrom::Start(header.fp_index as u64))?;
        let mut raw_index = vec![0u8; header.cb_index as usize];
        reader.read_exact(&mut raw_index)?;

        // Parse index as GG (General Group)
        let chunks = Self::parse_index(&raw_index, &header, swap)?;

        // Read chunk data
        let mut chunk_data = std::collections::HashMap::new();
        for entry in &chunks {
            if entry.cb > 0 {
                reader.seek(SeekFrom::Start(entry.fp as u64))?;
                let mut data = vec![0u8; entry.cb as usize];
                reader.read_exact(&mut data)?;
                chunk_data.insert((entry.id.ctg, entry.id.cno), data);
            }
        }

        // Read free space map
        let free_map = Vec::new();
        if header.cb_map > 0 {
            reader.seek(SeekFrom::Start(header.fp_map as u64))?;
            let mut map_buf = vec![0u8; header.cb_map as usize];
            reader.read_exact(&mut map_buf)?;
            // Parse as GL of FSM entries
            // TODO: implement GL deserialization for free map
        }

        Ok(ChunkyFile {
            header,
            chunks,
            chunk_data,
            free_map,
            raw_index,
        })
    }

    /// Get decompressed chunk data.
    pub fn get_chunk_data(&self, ctg: u32, cno: u32) -> Result<Vec<u8>> {
        let raw = self.chunk_data.get(&(ctg, cno))
            .ok_or(ChunkyError::ChunkNotFound { ctg, cno })?;

        // Find the chunk entry to check if it's compressed
        let entry = self.chunks.iter()
            .find(|c| c.id.ctg == ctg && c.id.cno == cno)
            .ok_or(ChunkyError::ChunkNotFound { ctg, cno })?;

        if entry.is_packed() {
            codec::decompress(raw, entry.cb as usize)
        } else {
            Ok(raw.clone())
        }
    }

    /// Serialize the entire file to bytes without modifying any chunk data.
    ///
    /// Chunks are written sequentially starting at offset 128 (after header).
    /// The GGF index is rebuilt with updated fp values.
    /// Compressed chunks are preserved verbatim (PACKED flag stays set).
    /// This is the foundation for the Phase 7 editor write path.
    pub fn to_bytes_passthrough(&self) -> Result<Vec<u8>> {
        let mut out: Vec<u8> = Vec::new();

        // ── 1. Placeholder header (128 bytes) ──────────────────────────────
        out.resize(HEADER_SIZE, 0u8);

        // ── 2. Write chunks sequentially; record new file positions ────────
        let mut new_fp: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
        for entry in &self.chunks {
            if entry.cb == 0 {
                new_fp.insert((entry.id.ctg, entry.id.cno), 0);
                continue;
            }
            let data = self.chunk_data
                .get(&(entry.id.ctg, entry.id.cno))
                .ok_or(ChunkyError::ChunkNotFound { ctg: entry.id.ctg, cno: entry.id.cno })?;
            let fp = out.len() as u32;
            new_fp.insert((entry.id.ctg, entry.id.cno), fp);
            out.extend_from_slice(data);
        }

        // ── 3. Build and write GGF index ────────────────────────────────────
        let fp_index = out.len() as u32;
        let use_small = self.header.uses_small_index();
        let ggf_bytes = Self::write_ggf_index(&self.chunks, &new_fp, self.header.byte_order, self.header.os_kind, use_small)?;
        let cb_index = ggf_bytes.len() as u32;
        out.extend_from_slice(&ggf_bytes);

        // ── 4. No free-space map (FSM) in passthrough ───────────────────────
        let fp_mac = out.len() as u32;

        // ── 5. Patch header ─────────────────────────────────────────────────
        let mut hdr = self.header.clone();
        hdr.fp_mac   = fp_mac;
        hdr.fp_index = fp_index;
        hdr.cb_index = cb_index;
        hdr.fp_map   = 0;
        hdr.cb_map   = 0;
        let mut hdr_buf = Vec::with_capacity(HEADER_SIZE);
        hdr.write(&mut hdr_buf)?;
        out[..HEADER_SIZE].copy_from_slice(&hdr_buf);

        Ok(out)
    }

    /// Rebuild a GGF (file-index format) from parsed ChunkEntry slice.
    ///
    /// Layout written:
    /// ```text
    /// [GGF header: 20]  bo+osk+ilocMac+bvMac+clocFree+cbFixed
    /// [data buffer]     each element at sequential bv offsets (fixed + variable)
    /// [LOC table]       ilocMac × {bv:u32, cb:u32}
    /// ```
    fn write_ggf_index(
        chunks: &[ChunkEntry],
        new_fp: &std::collections::HashMap<(u32, u32), u32>,
        bo: u16,
        osk: u16,
        use_small: bool,
    ) -> Result<Vec<u8>> {
        let cb_fixed: u32 = if use_small { ChunkEntry::SIZE_SMALL as u32 } else { ChunkEntry::SIZE_BIG as u32 };
        let iloc_mac = chunks.len() as u32;

        // Build data buffer sequentially
        let mut data_buf: Vec<u8> = Vec::new();
        let mut locs: Vec<(u32, u32)> = Vec::with_capacity(chunks.len());

        for entry in chunks {
            let bv = data_buf.len() as u32;

            // Fixed portion
            let fp_val = *new_fp.get(&(entry.id.ctg, entry.id.cno)).unwrap_or(&entry.fp);
            if use_small {
                let lu = (entry.cb << 8) | (entry.flags.bits() & 0xFF);
                data_buf.extend_from_slice(&entry.id.ctg.to_le_bytes());
                data_buf.extend_from_slice(&entry.id.cno.to_le_bytes());
                data_buf.extend_from_slice(&fp_val.to_le_bytes());
                data_buf.extend_from_slice(&lu.to_le_bytes());
                data_buf.extend_from_slice(&(entry.child_count as u16).to_le_bytes());
                data_buf.extend_from_slice(&(entry.ref_count as u16).to_le_bytes());
            } else {
                data_buf.extend_from_slice(&entry.id.ctg.to_le_bytes());
                data_buf.extend_from_slice(&entry.id.cno.to_le_bytes());
                data_buf.extend_from_slice(&fp_val.to_le_bytes());
                data_buf.extend_from_slice(&entry.cb.to_le_bytes());
                data_buf.extend_from_slice(&entry.child_count.to_le_bytes());
                data_buf.extend_from_slice(&entry.ref_count.to_le_bytes());
                data_buf.extend_from_slice(&entry.rti.to_le_bytes());
                data_buf.extend_from_slice(&entry.flags.bits().to_le_bytes());
            }

            // Variable portion: children
            for child in &entry.children {
                data_buf.extend_from_slice(&child.id.ctg.to_le_bytes());
                data_buf.extend_from_slice(&child.id.cno.to_le_bytes());
                data_buf.extend_from_slice(&child.chid.to_le_bytes());
            }
            // Optional name (STN)
            if let Some(name) = &entry.name {
                data_buf.extend_from_slice(&osk.to_le_bytes());
                data_buf.push(name.len() as u8);
                data_buf.extend_from_slice(name.as_bytes());
                data_buf.push(0u8); // null terminator
            }

            let cb = data_buf.len() as u32 - bv;
            locs.push((bv, cb));
        }

        let bv_mac = data_buf.len() as u32;

        // Assemble GGF
        let mut out = Vec::with_capacity(20 + bv_mac as usize + iloc_mac as usize * 8);
        // GGF header (20 bytes)
        out.extend_from_slice(&bo.to_le_bytes());
        out.extend_from_slice(&osk.to_le_bytes());
        out.extend_from_slice(&iloc_mac.to_le_bytes());
        out.extend_from_slice(&bv_mac.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // clocFree
        out.extend_from_slice(&cb_fixed.to_le_bytes());
        // Data buffer
        out.extend_from_slice(&data_buf);
        // LOC table
        for (bv, cb) in &locs {
            out.extend_from_slice(&bv.to_le_bytes());
            out.extend_from_slice(&cb.to_le_bytes());
        }

        Ok(out)
    }

    /// Find a chunk entry by type and number.
    pub fn find_chunk(&self, ctg: u32, cno: u32) -> Option<&ChunkEntry> {
        self.chunks.iter().find(|c| c.id.ctg == ctg && c.id.cno == cno)
    }

    /// Get children of a chunk.
    pub fn get_children(&self, ctg: u32, cno: u32) -> Vec<&ChildRef> {
        self.chunks.iter()
            .find(|c| c.id.ctg == ctg && c.id.cno == cno)
            .map(|c| c.children.iter().collect())
            .unwrap_or_default()
    }

    /// Parse the chunk index from raw GG bytes.
    ///
    /// On-disk layout (confirmed from Kauai groups.cpp / CbOnFile):
    ///   [GGF header: 20 bytes]
    ///   [data buffer: bvMac bytes]   ← fixed+variable data, GG-allocator-scattered
    ///   [LOC table:   ilocMac×8 bytes] ← {bv:u32, cb:u32} pairs into data buffer
    ///
    /// CbOnFile = 20 + bvMac + ilocMac*8.
    fn parse_index(raw: &[u8], header: &ChunkyHeader, _swap: bool) -> Result<Vec<ChunkEntry>> {
        if raw.len() < 20 {
            return Ok(Vec::new());
        }

        // ── GGF header (20 bytes) ─────────────────────────────────────────────
        let mut cursor = std::io::Cursor::new(raw);
        let _bo        = cursor.read_u16::<LittleEndian>()?;
        let _osk       = cursor.read_u16::<LittleEndian>()?;
        let iloc_mac   = cursor.read_u32::<LittleEndian>()? as usize; // element count
        let bv_mac     = cursor.read_u32::<LittleEndian>()? as usize; // data buffer size
        let _cloc_free = cursor.read_u32::<LittleEndian>()?;          // free-slot count (ignored)
        let cb_fixed   = cursor.read_u32::<LittleEndian>()? as usize; // fixed bytes per element

        // Data buffer: bytes [20 .. 20+bvMac)
        let buf_start = 20usize;
        let buf_end   = buf_start + bv_mac;
        // LOC table:  bytes [buf_end .. buf_end + ilocMac*8)
        let loc_start = buf_end;
        let loc_end   = loc_start + iloc_mac * 8;

        if raw.len() < loc_end {
            return Err(ChunkyError::IndexCorruption(
                format!("index too small: {} < {} required", raw.len(), loc_end)
            ));
        }

        let data_buf = &raw[buf_start..buf_end];
        let loc_buf  = &raw[loc_start..loc_end];

        let use_small = header.uses_small_index() && cb_fixed == ChunkEntry::SIZE_SMALL;
        let mut chunks = Vec::with_capacity(iloc_mac);

        for i in 0..iloc_mac {
            // ── Read LOC[i] ───────────────────────────────────────────────────
            let lo = i * 8;
            let bv  = u32::from_le_bytes(loc_buf[lo..lo+4].try_into().unwrap()) as usize;
            let cb  = u32::from_le_bytes(loc_buf[lo+4..lo+8].try_into().unwrap()) as usize;

            // Skip free / deleted slots
            if cb == 0 { continue; }

            // Bounds check
            if bv + cb > data_buf.len() || cb < cb_fixed {
                return Err(ChunkyError::IndexCorruption(
                    format!("LOC[{}]: bv={} cb={} out of bounds (buf={})", i, bv, cb, data_buf.len())
                ));
            }

            // ── Parse fixed part (CRPSM or CRPBG) ────────────────────────────
            let fixed_bytes = &data_buf[bv..bv + cb_fixed];
            let mut entry = if use_small {
                Self::parse_crpsm(fixed_bytes)?
            } else {
                Self::parse_crpbg(fixed_bytes)?
            };

            // ── Parse variable part (children + optional name) ─────────────
            let var_bytes = &data_buf[bv + cb_fixed .. bv + cb];
            entry.children = Self::parse_var_data(var_bytes, entry.child_count as usize, entry.name.is_some());
            // Try to parse name from remaining bytes after children
            let kids_cb = entry.child_count as usize * ChildRef::SIZE;
            if var_bytes.len() > kids_cb {
                entry.name = Self::parse_stn(&var_bytes[kids_cb..]);
            }

            chunks.push(entry);
        }

        Ok(chunks)
    }

    /// Parse children from variable data.
    fn parse_var_data(var: &[u8], kid_count: usize, _has_name: bool) -> Vec<ChildRef> {
        let mut children = Vec::with_capacity(kid_count);
        for k in 0..kid_count {
            let off = k * ChildRef::SIZE;
            if off + ChildRef::SIZE > var.len() { break; }
            let b = &var[off..off + ChildRef::SIZE];
            children.push(ChildRef {
                id: ChunkId {
                    ctg: u32::from_le_bytes(b[0..4].try_into().unwrap()),
                    cno: u32::from_le_bytes(b[4..8].try_into().unwrap()),
                },
                chid: u32::from_le_bytes(b[8..12].try_into().unwrap()),
            });
        }
        children
    }

    /// Parse an STN (short text name) from bytes.
    /// Format: [osk: u16][cch: u8][chars: cch bytes][null: 1 byte]
    fn parse_stn(bytes: &[u8]) -> Option<String> {
        if bytes.len() < 4 { return None; }
        let cch = bytes[2] as usize;
        if bytes.len() < 3 + cch + 1 { return None; }
        let chars = &bytes[3..3 + cch];
        Some(String::from_utf8_lossy(chars).into_owned())
    }

    /// Parse a CRPSM (small, 20-byte) index entry.
    fn parse_crpsm(data: &[u8]) -> Result<ChunkEntry> {
        let ctg = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let cno = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let fp = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let lu = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let ckid = u16::from_le_bytes([data[16], data[17]]);
        let ccrp_ref = u16::from_le_bytes([data[18], data[19]]);

        let flags = ChunkFlags::from_bits_truncate(lu & 0xFF);
        let cb = lu >> 8;

        Ok(ChunkEntry {
            id: ChunkId { ctg, cno },
            fp,
            cb,
            flags,
            child_count: ckid as u32,
            ref_count: ccrp_ref as u32,
            rti: 0,
            children: Vec::new(),
            name: None,
        })
    }

    /// Parse a CRPBG (big, 32-byte) index entry.
    fn parse_crpbg(data: &[u8]) -> Result<ChunkEntry> {
        let ctg = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let cno = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let fp = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let cb = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let ckid = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let ccrp_ref = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        let rti = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        let grfcrp = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);

        Ok(ChunkEntry {
            id: ChunkId { ctg, cno },
            fp,
            cb,
            flags: ChunkFlags::from_bits_truncate(grfcrp),
            child_count: ckid,
            ref_count: ccrp_ref,
            rti,
            children: Vec::new(),
            name: None,
        })
    }
}
