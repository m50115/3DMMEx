//! GL, GG, GST — Collection serialization matching original binary layout.

use byteorder::{LittleEndian, ReadBytesExt};
use crate::error::{ChunkyError, Result};

/// GL (General List) — fixed-size element collection.
///
/// On-disk layout:
/// ```text
/// [i32 cbEntry][i32 ivMac][i16 bo][i16 osk]
/// [data: ivMac * cbEntry bytes]
/// ```
#[derive(Debug, Clone)]
pub struct GenericList {
    /// Size of each entry in bytes
    pub entry_size: u32,
    /// Byte order
    pub bo: u16,
    /// OS kind
    pub osk: u16,
    /// Raw entry data
    pub entries: Vec<Vec<u8>>,
}

/// GG (General Group) — fixed + variable-size element collection.
///
/// On-disk layout (GGF, 20-byte header — matches Kauai `GGB::FWrite`):
/// ```text
/// [u16 bo][u16 osk][i32 ilocMac][i32 bvMac][i32 clocFree][i32 cbFixed]  // 20-byte header
/// [hqData1: bvMac bytes]   — entries packed as (cbFixed fixed bytes)(variable bytes)
/// [LOC array: ilocMac * 8 bytes]  — {i32 bv, i32 cb} per entry; bv = offset into hqData1
/// ```
/// Each LOC entry: bv = byte offset into hqData1; cb = cbFixed + variable_len.
#[derive(Debug, Clone)]
pub struct GenericGroup {
    /// Size of fixed portion per entry
    pub fixed_size: u32,
    /// Byte order
    pub bo: u16,
    /// OS kind
    pub osk: u16,
    /// Fixed-size data per entry
    pub fixed_entries: Vec<Vec<u8>>,
    /// Variable-size data per entry
    pub variable_entries: Vec<Vec<u8>>,
}

/// GST (Generic String Table) — string table with optional extra data per entry.
///
/// On-disk layout:
/// ```text
/// [i32 cbExtra][i32 istnMac][i16 bo][i16 osk]
/// [entries: istnMac * (4 + cbExtra) bytes]  -- {i32 bst, extra[cbExtra]}
/// [string data]
/// ```
#[derive(Debug, Clone)]
pub struct StringTable {
    /// Extra data size per entry
    pub extra_size: u32,
    /// Byte order
    pub bo: u16,
    /// OS kind
    pub osk: u16,
    /// Strings
    pub strings: Vec<String>,
    /// Extra data per string (if extra_size > 0)
    pub extras: Vec<Vec<u8>>,
}

/// Collection header — shared across GL, GG, GST (12 bytes).
#[derive(Debug, Clone, Copy)]
pub struct CollectionHeader {
    pub cb_entry: u32, // cbEntry for GL, cbFixed for GG, cbExtra for GST
    pub count: u32,    // ivMac or istnMac
    pub bo: u16,
    pub osk: u16,
}

impl CollectionHeader {
    pub const SIZE: usize = 12;

    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(ChunkyError::InvalidCollection(
                format!("Header too small: {} < {}", data.len(), Self::SIZE)
            ));
        }
        let mut c = std::io::Cursor::new(data);
        Ok(Self {
            cb_entry: c.read_u32::<LittleEndian>()?,
            count: c.read_u32::<LittleEndian>()?,
            bo: c.read_u16::<LittleEndian>()?,
            osk: c.read_u16::<LittleEndian>()?,
        })
    }
}

impl GenericList {
    /// Deserialize a GL from raw bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        let header = CollectionHeader::read(data)?;
        let entry_size = header.cb_entry as usize;
        let count = header.count as usize;
        let data_start = CollectionHeader::SIZE;
        let expected = data_start + count * entry_size;

        if data.len() < expected {
            return Err(ChunkyError::InvalidCollection(
                format!("GL data too small: {} < {}", data.len(), expected)
            ));
        }

        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let off = data_start + i * entry_size;
            entries.push(data[off..off + entry_size].to_vec());
        }

        Ok(Self {
            entry_size: header.cb_entry,
            bo: header.bo,
            osk: header.osk,
            entries,
        })
    }

    /// Serialize a GL to bytes.
    pub fn write(&self) -> Vec<u8> {
        let count = self.entries.len();
        let total = CollectionHeader::SIZE + count * self.entry_size as usize;
        let mut buf = Vec::with_capacity(total);

        // Header
        buf.extend_from_slice(&self.entry_size.to_le_bytes());
        buf.extend_from_slice(&(count as u32).to_le_bytes());
        buf.extend_from_slice(&self.bo.to_le_bytes());
        buf.extend_from_slice(&self.osk.to_le_bytes());

        // Data
        for entry in &self.entries {
            buf.extend_from_slice(entry);
        }

        buf
    }
}

/// GGF header size in bytes.
const GGF_HEADER_SIZE: usize = 20;

impl GenericGroup {
    /// Deserialize a GG from raw chunk bytes (GGF 20-byte header format).
    pub fn read(data: &[u8]) -> Result<Self> {
        if data.len() < GGF_HEADER_SIZE {
            return Err(ChunkyError::InvalidCollection(
                format!("GG header too small: {} < {GGF_HEADER_SIZE}", data.len())
            ));
        }
        let bo       = u16::from_le_bytes(data[0..2].try_into().unwrap());
        let osk      = u16::from_le_bytes(data[2..4].try_into().unwrap());
        let count    = i32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let bv_mac   = i32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        // data[12..16] = clocFree — ignored when reading
        let cb_fixed = i32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;

        let data1_start = GGF_HEADER_SIZE;
        let data1_end   = data1_start + bv_mac;
        let loc_start   = data1_end;
        let loc_end     = loc_start + count * 8;

        if data.len() < loc_end {
            return Err(ChunkyError::InvalidCollection(
                format!("GG data too small: {} < {loc_end}", data.len())
            ));
        }

        let data1 = &data[data1_start..data1_end];

        let mut fixed_entries    = Vec::with_capacity(count);
        let mut variable_entries = Vec::with_capacity(count);

        for i in 0..count {
            let loff = loc_start + i * 8;
            let bv   = i32::from_le_bytes(data[loff..loff+4].try_into().unwrap()) as usize;
            let cb   = i32::from_le_bytes(data[loff+4..loff+8].try_into().unwrap()) as usize;

            // Fixed: data1[bv .. bv+cb_fixed]
            let fixed = if bv + cb_fixed <= bv_mac {
                data1[bv..bv + cb_fixed].to_vec()
            } else {
                vec![0u8; cb_fixed]
            };

            // Variable: data1[bv+cb_fixed .. bv+cb]
            let var_end = bv + cb;
            let var = if cb > cb_fixed && var_end <= bv_mac {
                data1[bv + cb_fixed..var_end].to_vec()
            } else {
                Vec::new()
            };

            fixed_entries.push(fixed);
            variable_entries.push(var);
        }

        Ok(Self {
            fixed_size: cb_fixed as u32,
            bo,
            osk,
            fixed_entries,
            variable_entries,
        })
    }

    /// Serialize a GG to bytes (GGF 20-byte header format).
    pub fn write(&self) -> Vec<u8> {
        let count    = self.fixed_entries.len();
        let cb_fixed = self.fixed_size as usize;

        // Build hqData1: each entry = cbFixed fixed bytes || variable bytes.
        let mut data1 = Vec::new();
        let mut locs: Vec<(u32, u32)> = Vec::with_capacity(count);

        for (fe, ve) in self.fixed_entries.iter().zip(self.variable_entries.iter()) {
            let bv = data1.len() as u32;
            let cb = (cb_fixed + ve.len()) as u32;
            data1.extend_from_slice(fe);
            data1.extend_from_slice(ve);
            locs.push((bv, cb));
        }

        let bv_mac = data1.len() as u32;
        let mut buf = Vec::with_capacity(GGF_HEADER_SIZE + bv_mac as usize + count * 8);

        // 20-byte GGF header
        buf.extend_from_slice(&self.bo.to_le_bytes());
        buf.extend_from_slice(&self.osk.to_le_bytes());
        buf.extend_from_slice(&(count as i32).to_le_bytes());      // ilocMac
        buf.extend_from_slice(&(bv_mac as i32).to_le_bytes());     // bvMac
        buf.extend_from_slice(&(-1i32).to_le_bytes());             // clocFree = cvNil
        buf.extend_from_slice(&(cb_fixed as i32).to_le_bytes());   // cbFixed

        // hqData1
        buf.extend_from_slice(&data1);

        // LOC array
        for (bv, cb) in &locs {
            buf.extend_from_slice(&bv.to_le_bytes());
            buf.extend_from_slice(&cb.to_le_bytes());
        }

        buf
    }
}

impl StringTable {
    /// Deserialize a GST from raw bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        let header = CollectionHeader::read(data)?;
        let extra_size = header.cb_entry as usize;
        let count = header.count as usize;
        let entry_stride = 4 + extra_size; // bst offset + extra

        let entries_start = CollectionHeader::SIZE;
        let entries_end = entries_start + count * entry_stride;

        if data.len() < entries_end {
            return Err(ChunkyError::InvalidCollection(
                format!("GST data too small: {} < {}", data.len(), entries_end)
            ));
        }

        let string_data_start = entries_end;
        let mut strings = Vec::with_capacity(count);
        let mut extras = Vec::with_capacity(count);

        for i in 0..count {
            let eoff = entries_start + i * entry_stride;
            let bst = u32::from_le_bytes([
                data[eoff], data[eoff + 1], data[eoff + 2], data[eoff + 3],
            ]) as usize;

            if extra_size > 0 {
                extras.push(data[eoff + 4..eoff + 4 + extra_size].to_vec());
            } else {
                extras.push(Vec::new());
            }

            // Read string from string data area
            // STN format: first byte is length, then chars
            if string_data_start + bst < data.len() {
                let soff = string_data_start + bst;
                let slen = data[soff] as usize;
                if soff + 1 + slen <= data.len() {
                    let s = String::from_utf8_lossy(&data[soff + 1..soff + 1 + slen]).to_string();
                    strings.push(s);
                } else {
                    strings.push(String::new());
                }
            } else {
                strings.push(String::new());
            }
        }

        Ok(Self {
            extra_size: header.cb_entry,
            bo: header.bo,
            osk: header.osk,
            strings,
            extras,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bom;

    #[test]
    fn test_gl_roundtrip() {
        let gl = GenericList {
            entry_size: 8,
            bo: bom::BO_NATIVE,
            osk: bom::OSK_WIN,
            entries: vec![
                vec![1, 0, 0, 0, 2, 0, 0, 0],
                vec![3, 0, 0, 0, 4, 0, 0, 0],
            ],
        };

        let bytes = gl.write();
        let gl2 = GenericList::read(&bytes).unwrap();

        assert_eq!(gl2.entry_size, 8);
        assert_eq!(gl2.entries.len(), 2);
        assert_eq!(gl2.entries[0], vec![1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(gl2.entries[1], vec![3, 0, 0, 0, 4, 0, 0, 0]);
    }

    #[test]
    fn test_gg_roundtrip() {
        let gg = GenericGroup {
            fixed_size: 4,
            bo: bom::BO_NATIVE,
            osk: bom::OSK_WIN,
            fixed_entries: vec![
                vec![1, 0, 0, 0],
                vec![2, 0, 0, 0],
            ],
            variable_entries: vec![
                vec![0xAA, 0xBB],
                vec![0xCC, 0xDD, 0xEE],
            ],
        };

        let bytes = gg.write();
        let gg2 = GenericGroup::read(&bytes).unwrap();

        assert_eq!(gg2.fixed_size, 4);
        assert_eq!(gg2.fixed_entries.len(), 2);
        assert_eq!(gg2.variable_entries[0], vec![0xAA, 0xBB]);
        assert_eq!(gg2.variable_entries[1], vec![0xCC, 0xDD, 0xEE]);
    }
}
