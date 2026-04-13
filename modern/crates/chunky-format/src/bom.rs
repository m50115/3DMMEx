//! Byte Order Map (BOM) — describes struct field layout for byte-swapping.
//!
//! A BOM is a u32 bitmask read 2 bits at a time from MSB:
//! - 00: end of fields
//! - 01: i16 (2 bytes) — swap 2 bytes
//! - 10: i32 (4 bytes) — swap 4 bytes
//! - 11: i32 (4 bytes) — swap 4 bytes

/// Native byte order marker
pub const BO_NATIVE: u16 = 0x0001;
/// Swapped byte order marker
pub const BO_OTHER: u16 = 0x0100;

/// OS kind: Windows
pub const OSK_WIN: u16 = 0x7769; // 'wi'
/// OS kind: Macintosh
pub const OSK_MAC: u16 = 0x6D61; // 'ma'

/// Check if data needs byte-swapping based on the `bo` field.
#[inline]
pub fn needs_swap(bo: u16) -> bool {
    bo == BO_OTHER
}

/// Swap bytes in a buffer according to a BOM (Byte Order Map).
///
/// The BOM encodes field sizes as 2-bit pairs from MSB:
/// - 01 = 2-byte field (swap bytes)
/// - 10 or 11 = 4-byte field (swap bytes)
/// - 00 = end
pub fn swap_bytes_bom(data: &mut [u8], bom: u32) {
    let mut offset = 0;
    let mut mask = bom;

    loop {
        let bits = (mask >> 30) & 0x03;
        mask <<= 2;

        match bits {
            0b00 => break,
            0b01 => {
                // 2-byte field
                if offset + 1 < data.len() {
                    data.swap(offset, offset + 1);
                }
                offset += 2;
            }
            0b10 | 0b11 => {
                // 4-byte field
                if offset + 3 < data.len() {
                    data.swap(offset, offset + 3);
                    data.swap(offset + 1, offset + 2);
                }
                offset += 4;
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_swap() {
        assert!(!needs_swap(BO_NATIVE));
        assert!(needs_swap(BO_OTHER));
    }

    #[test]
    fn test_swap_bytes_bom_i32_i32() {
        // BOM: two i32 fields = 11 11 00... = 0xF0000000
        let mut data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        swap_bytes_bom(&mut data, 0xF000_0000);
        assert_eq!(data, [0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05]);
    }

    #[test]
    fn test_swap_bytes_bom_i16_i16() {
        // BOM: two i16 fields = 01 01 00... = 0x50000000
        let mut data = [0x01, 0x02, 0x03, 0x04];
        swap_bytes_bom(&mut data, 0x5000_0000);
        assert_eq!(data, [0x02, 0x01, 0x04, 0x03]);
    }

    #[test]
    fn test_swap_bytes_bom_mixed() {
        // BOM: i32 i16 i16 = 11 01 01 00 = 0xD4000000
        let mut data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        swap_bytes_bom(&mut data, 0xD400_0000);
        assert_eq!(data, [0x04, 0x03, 0x02, 0x01, 0x06, 0x05, 0x08, 0x07]);
    }
}
