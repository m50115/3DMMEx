//! BRender pixelmap / TMAP format.
//!
//! On-disk layout (TMAPF, 20 bytes):
//! ```text
//! [bo:i16][osk:i16][cbRow:i16][type:u8][grftmap:u8]
//! [xpLeft:i16][ypTop:i16][dxp:i16][dyp:i16][xpOrigin:i16][ypOrigin:i16]
//! ```
//! Immediately following the 20-byte header: `cbRow × dyp` bytes of pixel data.
//!
//! Source: `bren/inc/tmap.h`, `bren/tmap.cpp`.
//! Struct size verified by: `VERIFY_STRUCT_SIZE(TMAPF, 20)`.

use crate::error::{EngineError, EngineResult};

// ── Pixel type constants (BR_PMT_*) ─────────────────────────────────────────

/// 8-bit palettized index (most common in 3DMM content).
/// Actual value = 3 (enum starts INDEX_1=0, INDEX_2=1, INDEX_4=2, INDEX_8=3).
pub const BR_PMT_INDEX_8: u8 = 3;
/// 16-bit 5-5-5 RGB (1 bit ignored).
pub const BR_PMT_RGB_555: u8 = 4;
/// 16-bit 5-6-5 RGB.
pub const BR_PMT_RGB_565: u8 = 5;
/// 24-bit RGB.
pub const BR_PMT_RGB_888: u8 = 6;
/// 32-bit RGB + 8 bits padding (X).
pub const BR_PMT_RGBX_888: u8 = 7;
/// 32-bit RGBA.
pub const BR_PMT_RGBA_8888: u8 = 8;

// ── BrPixelType enum ─────────────────────────────────────────────────────────

/// Pixel format for a BRender pixelmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrPixelType {
    /// 8-bit palettized index.
    Index8,
    /// 16-bit 5-5-5 RGB.
    Rgb555,
    /// 16-bit 5-6-5 RGB.
    Rgb565,
    /// 24-bit RGB.
    Rgb888,
    /// 32-bit RGB + padding.
    RgbX888,
    /// 32-bit RGBA.
    Rgba8888,
    /// Unrecognized format code.
    Unknown(u8),
}

impl BrPixelType {
    pub fn from_byte(b: u8) -> Self {
        match b {
            BR_PMT_INDEX_8   => Self::Index8,
            BR_PMT_RGB_555   => Self::Rgb555,
            BR_PMT_RGB_565   => Self::Rgb565,
            BR_PMT_RGB_888   => Self::Rgb888,
            BR_PMT_RGBX_888  => Self::RgbX888,
            BR_PMT_RGBA_8888 => Self::Rgba8888,
            other            => Self::Unknown(other),
        }
    }

    /// Raw bytes per pixel for this format.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Index8               => 1,
            Self::Rgb555 | Self::Rgb565 => 2,
            Self::Rgb888               => 3,
            Self::RgbX888 | Self::Rgba8888 => 4,
            Self::Unknown(_)           => 1,
        }
    }
}

// ── BrTmap ───────────────────────────────────────────────────────────────────

/// Parsed BRender pixelmap (TMAPF on-disk format).
///
/// Pixel data is stored raw (as on disk). To convert to GPU-friendly RGBA8,
/// use `renderer::convert::tmap_to_rgba()`.
#[derive(Debug, Clone)]
pub struct BrTmap {
    /// Bytes per row, may be larger than `width × bpp` for alignment.
    pub row_bytes: i16,
    /// Raw pixel type code (use `pixel_type_parsed()` for enum).
    pub pixel_type: u8,
    /// Pixelmap flags (`BR_PMF_*`).
    pub flags: u8,
    /// Left edge of the active region within the buffer.
    pub base_x: i16,
    /// Top edge of the active region within the buffer.
    pub base_y: i16,
    /// Width in pixels of the active region.
    pub width: i16,
    /// Height in pixels.
    pub height: i16,
    /// X drawing origin offset.
    pub origin_x: i16,
    /// Y drawing origin offset.
    pub origin_y: i16,
    /// Raw pixel data: `row_bytes × height` bytes.
    pub pixels: Vec<u8>,
}

impl BrTmap {
    /// Size of the on-disk TMAPF header in bytes.
    pub const HEADER_SIZE: usize = 20;

    /// Parse a TMAPF chunk from a byte slice.
    ///
    /// The slice must contain at least the 20-byte header followed by
    /// `cbRow × dyp` bytes of pixel data.
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        if data.len() < Self::HEADER_SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "TMAPF header",
                need: Self::HEADER_SIZE,
                got:  data.len(),
            });
        }

        let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
        if bo != 0x0001 {
            return Err(EngineError::InvalidByteOrder(bo as u16));
        }
        // [2..4] = osk — ignored
        let row_bytes  = i16::from_le_bytes(data[4..6].try_into().unwrap());
        let pixel_type = data[6];
        let flags      = data[7];
        let base_x     = i16::from_le_bytes(data[8..10].try_into().unwrap());
        let base_y     = i16::from_le_bytes(data[10..12].try_into().unwrap());
        let width      = i16::from_le_bytes(data[12..14].try_into().unwrap());
        let height     = i16::from_le_bytes(data[14..16].try_into().unwrap());
        let origin_x   = i16::from_le_bytes(data[16..18].try_into().unwrap());
        let origin_y   = i16::from_le_bytes(data[18..20].try_into().unwrap());

        let pixel_len = (row_bytes.max(0) as usize) * (height.max(0) as usize);
        let required  = Self::HEADER_SIZE + pixel_len;

        if data.len() < required {
            return Err(EngineError::UnexpectedEof {
                what: "TMAPF pixel data",
                need: required,
                got:  data.len(),
            });
        }

        let pixels = data[Self::HEADER_SIZE..Self::HEADER_SIZE + pixel_len].to_vec();

        Ok(Self { row_bytes, pixel_type, flags, base_x, base_y, width, height, origin_x, origin_y, pixels })
    }

    /// Serialize back to on-disk TMAPF bytes (round-trip).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::HEADER_SIZE + self.pixels.len());
        b.extend_from_slice(&1i16.to_le_bytes());            // bo  = 0x0001 (LE)
        b.extend_from_slice(&0i16.to_le_bytes());            // osk = 0
        b.extend_from_slice(&self.row_bytes.to_le_bytes());
        b.push(self.pixel_type);
        b.push(self.flags);
        b.extend_from_slice(&self.base_x.to_le_bytes());
        b.extend_from_slice(&self.base_y.to_le_bytes());
        b.extend_from_slice(&self.width.to_le_bytes());
        b.extend_from_slice(&self.height.to_le_bytes());
        b.extend_from_slice(&self.origin_x.to_le_bytes());
        b.extend_from_slice(&self.origin_y.to_le_bytes());
        b.extend_from_slice(&self.pixels);
        b
    }

    /// Return the parsed pixel format enum.
    pub fn pixel_type_parsed(&self) -> BrPixelType {
        BrPixelType::from_byte(self.pixel_type)
    }

    /// Total number of pixels in the active region (`width × height`).
    pub fn pixel_count(&self) -> usize {
        self.width.max(0) as usize * self.height.max(0) as usize
    }

    /// Length of the pixel data buffer (`row_bytes × height`).
    pub fn pixel_data_len(&self) -> usize {
        self.row_bytes.max(0) as usize * self.height.max(0) as usize
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_1x1_index8() -> Vec<u8> {
        let mut b = vec![0u8; BrTmap::HEADER_SIZE + 1];
        // bo = 0x0001 (LE)
        b[0] = 0x01; b[1] = 0x00;
        // cbRow = 1, type = 3 (INDEX_8), flags = 0
        b[4] = 0x01; b[5] = 0x00; // cbRow = 1
        b[6] = BR_PMT_INDEX_8;     // type = 3
        b[7] = 0x00;               // flags
        // dxp = 1, dyp = 1
        b[12] = 0x01; b[13] = 0x00; // width = 1
        b[14] = 0x01; b[15] = 0x00; // height = 1
        // pixel at offset 20 = 0xAB
        b[20] = 0xAB;
        b
    }

    #[test]
    fn parse_1x1_index8() {
        let data = make_1x1_index8();
        let tmap = BrTmap::from_bytes(&data).unwrap();
        assert_eq!(tmap.width, 1);
        assert_eq!(tmap.height, 1);
        assert_eq!(tmap.row_bytes, 1);
        assert_eq!(tmap.pixel_type, BR_PMT_INDEX_8);
        assert_eq!(tmap.pixels, vec![0xAB]);
        assert_eq!(tmap.pixel_type_parsed(), BrPixelType::Index8);
    }

    #[test]
    fn roundtrip_1x1() {
        let data = make_1x1_index8();
        let tmap = BrTmap::from_bytes(&data).unwrap();
        let back = tmap.to_bytes();
        assert_eq!(&back[..], &data[..]);
    }

    #[test]
    fn pixel_count_and_data_len() {
        let data = make_1x1_index8();
        let tmap = BrTmap::from_bytes(&data).unwrap();
        assert_eq!(tmap.pixel_count(), 1);
        assert_eq!(tmap.pixel_data_len(), 1);
    }

    #[test]
    fn wrong_byte_order_rejected() {
        let mut data = make_1x1_index8();
        data[0] = 0x00; data[1] = 0x01; // bo = 0x0100 (BE, wrong)
        let result = BrTmap::from_bytes(&data);
        assert!(matches!(result, Err(EngineError::InvalidByteOrder(_))));
    }

    #[test]
    fn too_short_header_rejected() {
        let data = vec![0u8; 10]; // less than 20 bytes
        let result = BrTmap::from_bytes(&data);
        assert!(matches!(result, Err(EngineError::UnexpectedEof { .. })));
    }

    #[test]
    fn truncated_pixel_data_rejected() {
        let mut data = make_1x1_index8();
        data.pop(); // remove the one pixel byte
        let result = BrTmap::from_bytes(&data);
        assert!(matches!(result, Err(EngineError::UnexpectedEof { .. })));
    }

    #[test]
    fn pixel_type_enum_mapping() {
        assert_eq!(BrPixelType::from_byte(3), BrPixelType::Index8);
        assert_eq!(BrPixelType::from_byte(4), BrPixelType::Rgb555);
        assert_eq!(BrPixelType::from_byte(5), BrPixelType::Rgb565);
        assert_eq!(BrPixelType::from_byte(6), BrPixelType::Rgb888);
        assert_eq!(BrPixelType::from_byte(7), BrPixelType::RgbX888);
        assert_eq!(BrPixelType::from_byte(8), BrPixelType::Rgba8888);
        assert_eq!(BrPixelType::from_byte(99), BrPixelType::Unknown(99));
    }

    #[test]
    fn bytes_per_pixel() {
        assert_eq!(BrPixelType::Index8.bytes_per_pixel(), 1);
        assert_eq!(BrPixelType::Rgb555.bytes_per_pixel(), 2);
        assert_eq!(BrPixelType::Rgb565.bytes_per_pixel(), 2);
        assert_eq!(BrPixelType::Rgb888.bytes_per_pixel(), 3);
        assert_eq!(BrPixelType::RgbX888.bytes_per_pixel(), 4);
        assert_eq!(BrPixelType::Rgba8888.bytes_per_pixel(), 4);
    }

    #[test]
    fn parse_4x4_with_row_stride() {
        // 4×4 image, cbRow=8 (padded to 8 even though width=4 for INDEX_8)
        let pixel_data_len = 8 * 4; // cbRow × height = 32
        let mut data = vec![0u8; BrTmap::HEADER_SIZE + pixel_data_len];
        data[0] = 0x01; // bo LE
        data[4] = 0x08; // cbRow = 8
        data[6] = 0x00; // INDEX_8
        data[12] = 0x04; // width = 4
        data[14] = 0x04; // height = 4
        // Fill pixel data with pattern
        for i in 0..pixel_data_len {
            data[BrTmap::HEADER_SIZE + i] = i as u8;
        }
        let tmap = BrTmap::from_bytes(&data).unwrap();
        assert_eq!(tmap.width, 4);
        assert_eq!(tmap.height, 4);
        assert_eq!(tmap.row_bytes, 8);
        assert_eq!(tmap.pixels.len(), 32);
        assert_eq!(tmap.pixel_count(), 16);
        assert_eq!(tmap.pixel_data_len(), 32);
    }
}
