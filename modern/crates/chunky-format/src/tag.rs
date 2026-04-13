//! Resource Tag (TAG) — reference to a chunk in a specific file.

/// On-disk TAG format (TAGF) — 12 bytes.
///
/// References a chunk by source file ID + chunk type + chunk number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceTag {
    /// Source/file identifier
    pub sid: i32,
    /// Chunk type (CTG)
    pub ctg: u32,
    /// Chunk number (CNO)
    pub cno: u32,
}

impl ResourceTag {
    pub const SIZE: usize = 12;

    /// BOM for byte-swapping: three i32 fields = 11 11 11 00 = 0xFC000000
    pub const BOM: u32 = 0xFC00_0000;

    pub fn read(data: &[u8]) -> Self {
        use byteorder::{LittleEndian, ReadBytesExt};
        let mut cursor = std::io::Cursor::new(data);
        Self {
            sid: cursor.read_i32::<LittleEndian>().unwrap_or(0),
            ctg: cursor.read_u32::<LittleEndian>().unwrap_or(0),
            cno: cursor.read_u32::<LittleEndian>().unwrap_or(0),
        }
    }

    pub fn write(&self, buf: &mut [u8]) {
        use byteorder::{LittleEndian, WriteBytesExt};
        let mut cursor = std::io::Cursor::new(buf);
        cursor.write_i32::<LittleEndian>(self.sid).unwrap();
        cursor.write_u32::<LittleEndian>(self.ctg).unwrap();
        cursor.write_u32::<LittleEndian>(self.cno).unwrap();
    }
}
