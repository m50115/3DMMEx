//! BRender model (MODL chunk) domain types and parsing.
//!
//! On-disk layout:
//!   [MODLF header: 48 bytes]
//!   [br_vertex × cver: 32 bytes each]
//!   [br_face_file × cfac: 32 bytes each]
//!
//! All scalar values are BRS (i32 16.16 fixed-point).
//! Normals are br_fraction (i16, range [-1,1) via /32768).

use crate::error::{EngineError, EngineResult};
use crate::fixedpoint::FixedScalar;
use crate::transform::Vec3;

// ── Constants ───────────────────────────────────────────────────────────────

/// Expected byte-order marker for current platform (little-endian).
const BO_LITTLE_ENDIAN: i16 = 0x0001;

// ── Bounds — 24 bytes ──────────────────────────────────────────────────────

/// BRender bounding box (BRB / br_bounds). 24 bytes: min(3×BRS) + max(3×BRS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub const SIZE: usize = 24;

    pub fn from_le_bytes(b: &[u8; 24]) -> Self {
        Self {
            min: Vec3::from_le_bytes(b[0..12].try_into().unwrap()),
            max: Vec3::from_le_bytes(b[12..24].try_into().unwrap()),
        }
    }

    pub fn to_le_bytes(self) -> [u8; 24] {
        let mut b = [0u8; 24];
        b[0..12].copy_from_slice(&self.min.to_le_bytes());
        b[12..24].copy_from_slice(&self.max.to_le_bytes());
        b
    }
}

// ── ModelHeader (MODLF) — 48 bytes ────────────────────────────────────────

/// On-disk model header. 48 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelHeader {
    pub bo: i16,
    pub osk: i16,
    pub vertex_count: i16,
    pub face_count: i16,
    pub radius: FixedScalar,
    pub bounds: Bounds,
    pub pivot: Vec3,
}

impl ModelHeader {
    pub const SIZE: usize = 48;

    pub fn from_le_bytes(b: &[u8]) -> EngineResult<Self> {
        if b.len() < Self::SIZE {
            return Err(EngineError::UnexpectedEof {
                what: "MODLF header",
                need: Self::SIZE,
                got: b.len(),
            });
        }
        Ok(Self {
            bo: i16::from_le_bytes(b[0..2].try_into().unwrap()),
            osk: i16::from_le_bytes(b[2..4].try_into().unwrap()),
            vertex_count: i16::from_le_bytes(b[4..6].try_into().unwrap()),
            face_count: i16::from_le_bytes(b[6..8].try_into().unwrap()),
            radius: FixedScalar(i32::from_le_bytes(b[8..12].try_into().unwrap())),
            bounds: Bounds::from_le_bytes(b[12..36].try_into().unwrap()),
            pivot: Vec3::from_le_bytes(b[36..48].try_into().unwrap()),
        })
    }

    pub fn to_le_bytes(self) -> [u8; 48] {
        let mut b = [0u8; 48];
        b[0..2].copy_from_slice(&self.bo.to_le_bytes());
        b[2..4].copy_from_slice(&self.osk.to_le_bytes());
        b[4..6].copy_from_slice(&self.vertex_count.to_le_bytes());
        b[6..8].copy_from_slice(&self.face_count.to_le_bytes());
        b[8..12].copy_from_slice(&self.radius.0.to_le_bytes());
        b[12..36].copy_from_slice(&self.bounds.to_le_bytes());
        b[36..48].copy_from_slice(&self.pivot.to_le_bytes());
        b
    }
}

// ── BrVertex — 32 bytes ───────────────────────────────────────────────────

/// BRender vertex as stored on disk. 32 bytes.
///
/// Layout:
///   position:  3 × BRS (i32)  = 12 bytes
///   uv:        2 × BRS (i32)  =  8 bytes
///   index:     u8              =  1 byte  (prelit color index)
///   red:       u8              =  1 byte
///   green:     u8              =  1 byte
///   blue:      u8              =  1 byte
///   _reserved: u16             =  2 bytes
///   normal:    3 × i16         =  6 bytes (br_fraction: value/32768)
///   Total                      = 32 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrVertex {
    pub position: Vec3,
    pub uv: [FixedScalar; 2],
    pub index: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub normal: [i16; 3],
}

impl BrVertex {
    pub const SIZE: usize = 32;

    pub fn from_le_bytes(b: &[u8; 32]) -> Self {
        Self {
            position: Vec3::from_le_bytes(b[0..12].try_into().unwrap()),
            uv: [
                FixedScalar(i32::from_le_bytes(b[12..16].try_into().unwrap())),
                FixedScalar(i32::from_le_bytes(b[16..20].try_into().unwrap())),
            ],
            index: b[20],
            red: b[21],
            green: b[22],
            blue: b[23],
            // b[24..26] = _reserved (u16), skip
            normal: [
                i16::from_le_bytes(b[26..28].try_into().unwrap()),
                i16::from_le_bytes(b[28..30].try_into().unwrap()),
                i16::from_le_bytes(b[30..32].try_into().unwrap()),
            ],
        }
    }

    pub fn to_le_bytes(self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..12].copy_from_slice(&self.position.to_le_bytes());
        b[12..16].copy_from_slice(&self.uv[0].0.to_le_bytes());
        b[16..20].copy_from_slice(&self.uv[1].0.to_le_bytes());
        b[20] = self.index;
        b[21] = self.red;
        b[22] = self.green;
        b[23] = self.blue;
        // b[24..26] = reserved, left as zero
        b[26..28].copy_from_slice(&self.normal[0].to_le_bytes());
        b[28..30].copy_from_slice(&self.normal[1].to_le_bytes());
        b[30..32].copy_from_slice(&self.normal[2].to_le_bytes());
        b
    }
}

// ── BrFaceFile — 32 bytes ─────────────────────────────────────────────────

/// BRender face as stored on disk (br_face_file / BRFF). 32 bytes.
///
/// Layout:
///   vertices:  3 × u16  =  6 bytes
///   edges:     3 × u16  =  6 bytes
///   material:  u32       =  4 bytes  (index on disk, pointer at runtime)
///   smoothing: u16       =  2 bytes
///   flags:     u8        =  1 byte
///   _pad0:     u8        =  1 byte
///   normal:    3 × i16   =  6 bytes  (br_fvector3: fraction normals)
///   _pad1:     u16       =  2 bytes  (alignment padding before d)
///   d:         i32       =  4 bytes  (plane offset, BRS)
///   Total                = 32 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrFaceFile {
    pub vertices: [u16; 3],
    pub edges: [u16; 3],
    pub material: u32,
    pub smoothing: u16,
    pub flags: u8,
    pub normal: [i16; 3],
    pub d: FixedScalar,
}

impl BrFaceFile {
    pub const SIZE: usize = 32;

    pub fn from_le_bytes(b: &[u8; 32]) -> Self {
        Self {
            vertices: [
                u16::from_le_bytes(b[0..2].try_into().unwrap()),
                u16::from_le_bytes(b[2..4].try_into().unwrap()),
                u16::from_le_bytes(b[4..6].try_into().unwrap()),
            ],
            edges: [
                u16::from_le_bytes(b[6..8].try_into().unwrap()),
                u16::from_le_bytes(b[8..10].try_into().unwrap()),
                u16::from_le_bytes(b[10..12].try_into().unwrap()),
            ],
            material: u32::from_le_bytes(b[12..16].try_into().unwrap()),
            smoothing: u16::from_le_bytes(b[16..18].try_into().unwrap()),
            flags: b[18],
            // b[19] = _pad0
            normal: [
                i16::from_le_bytes(b[20..22].try_into().unwrap()),
                i16::from_le_bytes(b[22..24].try_into().unwrap()),
                i16::from_le_bytes(b[24..26].try_into().unwrap()),
            ],
            // b[26..28] = alignment padding
            d: FixedScalar(i32::from_le_bytes(b[28..32].try_into().unwrap())),
        }
    }

    pub fn to_le_bytes(self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..2].copy_from_slice(&self.vertices[0].to_le_bytes());
        b[2..4].copy_from_slice(&self.vertices[1].to_le_bytes());
        b[4..6].copy_from_slice(&self.vertices[2].to_le_bytes());
        b[6..8].copy_from_slice(&self.edges[0].to_le_bytes());
        b[8..10].copy_from_slice(&self.edges[1].to_le_bytes());
        b[10..12].copy_from_slice(&self.edges[2].to_le_bytes());
        b[12..16].copy_from_slice(&self.material.to_le_bytes());
        b[16..18].copy_from_slice(&self.smoothing.to_le_bytes());
        b[18] = self.flags;
        // b[19] = _pad0, left as zero
        b[20..22].copy_from_slice(&self.normal[0].to_le_bytes());
        b[22..24].copy_from_slice(&self.normal[1].to_le_bytes());
        b[24..26].copy_from_slice(&self.normal[2].to_le_bytes());
        // b[26..28] = alignment padding, left as zero
        b[28..32].copy_from_slice(&self.d.0.to_le_bytes());
        b
    }
}

// ── Model ──────────────────────────────────────────────────────────────────

/// Parsed BRender model: header + vertex array + face array.
///
/// Models come in two flavours on disk:
/// - **Prepared** (`radius > 0`): pre-processed by BRender. Face indices are
///   valid references into the vertex array.
/// - **Unprepared** (`radius == 0`): raw content-tool output. Face data may
///   contain runtime garbage (pointer values, uninitialised fields). Vertex
///   positions are still usable but face indices are NOT reliable.
#[derive(Debug, Clone)]
pub struct Model {
    pub header: ModelHeader,
    pub vertices: Vec<BrVertex>,
    pub faces: Vec<BrFaceFile>,
}

impl Model {
    /// Parse a MODL/BMDL chunk from raw bytes (after decompression).
    ///
    /// Expected layout: `[MODLF:48][BrVertex × cver][BrFaceFile × cfac]`
    ///
    /// Returns `Ok` for any structurally valid chunk (correct size, bo=LE).
    /// Use [`has_valid_faces`] to check whether face data is usable.
    pub fn from_bytes(data: &[u8]) -> EngineResult<Self> {
        let header = ModelHeader::from_le_bytes(data)?;

        if header.bo != BO_LITTLE_ENDIAN {
            return Err(EngineError::InvalidByteOrder(header.bo as u16));
        }

        let cver = header.vertex_count as usize;
        let cfac = header.face_count as usize;
        let expected = ModelHeader::SIZE + cver * BrVertex::SIZE + cfac * BrFaceFile::SIZE;

        if data.len() < expected {
            return Err(EngineError::UnexpectedEof {
                what: "MODL chunk",
                need: expected,
                got: data.len(),
            });
        }

        let vert_start = ModelHeader::SIZE;
        let mut vertices = Vec::with_capacity(cver);
        for i in 0..cver {
            let off = vert_start + i * BrVertex::SIZE;
            let chunk: &[u8; 32] = data[off..off + 32].try_into().unwrap();
            vertices.push(BrVertex::from_le_bytes(chunk));
        }

        let face_start = vert_start + cver * BrVertex::SIZE;
        let mut faces = Vec::with_capacity(cfac);
        for i in 0..cfac {
            let off = face_start + i * BrFaceFile::SIZE;
            let chunk: &[u8; 32] = data[off..off + 32].try_into().unwrap();
            faces.push(BrFaceFile::from_le_bytes(chunk));
        }

        Ok(Self { header, vertices, faces })
    }

    /// Whether this model was pre-prepared by BRender (radius > 0).
    ///
    /// Only prepared models have reliable face vertex indices. Unprepared
    /// models (radius == 0) were dumped from the content pipeline before
    /// BrModelPrepare; their face data contains runtime pointer values.
    pub fn is_prepared(&self) -> bool {
        self.header.radius.0 > 0
    }

    /// Check that every face vertex index is within the vertex array.
    pub fn has_valid_faces(&self) -> bool {
        let nv = self.header.vertex_count as u16;
        self.faces.iter()
            .all(|f| f.vertices.iter().all(|&vi| vi < nv))
    }

    /// Serialize back to bytes (for round-trip testing).
    pub fn to_bytes(&self) -> Vec<u8> {
        let cver = self.vertices.len();
        let cfac = self.faces.len();
        let mut buf = Vec::with_capacity(ModelHeader::SIZE + cver * BrVertex::SIZE + cfac * BrFaceFile::SIZE);

        buf.extend_from_slice(&self.header.to_le_bytes());
        for v in &self.vertices {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        for f in &self.faces {
            buf.extend_from_slice(&f.to_le_bytes());
        }
        buf
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vertex() -> BrVertex {
        BrVertex {
            position: Vec3 {
                x: FixedScalar(0x0001_0000), // 1.0
                y: FixedScalar(0x0002_0000), // 2.0
                z: FixedScalar(0x0003_0000), // 3.0
            },
            uv: [
                FixedScalar(0x0000_8000), // 0.5
                FixedScalar(0x0000_C000), // 0.75
            ],
            index: 0,
            red: 255,
            green: 128,
            blue: 64,
            normal: [0, 0x7FFF, 0], // (0, ~1.0, 0) pointing up
        }
    }

    fn sample_face() -> BrFaceFile {
        BrFaceFile {
            vertices: [0, 1, 2],
            edges: [0, 1, 2],
            material: 0,
            smoothing: 1,
            flags: 0,
            normal: [0, 0x7FFF, 0],
            d: FixedScalar(0x0001_0000),
        }
    }

    fn sample_header(cver: i16, cfac: i16) -> ModelHeader {
        ModelHeader {
            bo: BO_LITTLE_ENDIAN,
            osk: 0x7769, // 'wi' = Windows
            vertex_count: cver,
            face_count: cfac,
            radius: FixedScalar(0x0005_0000), // 5.0
            bounds: Bounds {
                min: Vec3 {
                    x: FixedScalar(-0x0001_0000),
                    y: FixedScalar(-0x0001_0000),
                    z: FixedScalar(-0x0001_0000),
                },
                max: Vec3 {
                    x: FixedScalar(0x0001_0000),
                    y: FixedScalar(0x0001_0000),
                    z: FixedScalar(0x0001_0000),
                },
            },
            pivot: Vec3::ZERO,
        }
    }

    #[test]
    fn vertex_roundtrip() {
        let v = sample_vertex();
        let bytes = v.to_le_bytes();
        assert_eq!(bytes.len(), BrVertex::SIZE);
        let v2 = BrVertex::from_le_bytes(&bytes);
        assert_eq!(v, v2);
    }

    #[test]
    fn vertex_size_is_32() {
        assert_eq!(BrVertex::SIZE, 32);
    }

    #[test]
    fn face_roundtrip() {
        let f = sample_face();
        let bytes = f.to_le_bytes();
        assert_eq!(bytes.len(), BrFaceFile::SIZE);
        let f2 = BrFaceFile::from_le_bytes(&bytes);
        assert_eq!(f, f2);
    }

    #[test]
    fn face_size_is_32() {
        assert_eq!(BrFaceFile::SIZE, 32);
    }

    #[test]
    fn header_roundtrip() {
        let h = sample_header(3, 1);
        let bytes = h.to_le_bytes();
        assert_eq!(bytes.len(), ModelHeader::SIZE);
        let h2 = ModelHeader::from_le_bytes(&bytes).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn model_roundtrip() {
        let model = Model {
            header: sample_header(3, 1),
            vertices: vec![sample_vertex(); 3],
            faces: vec![sample_face()],
        };

        let bytes = model.to_bytes();
        let expected_len = 48 + 3 * 32 + 1 * 32;
        assert_eq!(bytes.len(), expected_len);

        let model2 = Model::from_bytes(&bytes).unwrap();
        assert_eq!(model2.header, model.header);
        assert_eq!(model2.vertices.len(), 3);
        assert_eq!(model2.faces.len(), 1);
        assert_eq!(model2.vertices[0], model.vertices[0]);
        assert_eq!(model2.faces[0], model.faces[0]);
    }

    #[test]
    fn model_rejects_truncated_data() {
        let model = Model {
            header: sample_header(3, 1),
            vertices: vec![sample_vertex(); 3],
            faces: vec![sample_face()],
        };
        let bytes = model.to_bytes();

        // Truncate: remove last face
        let truncated = &bytes[..bytes.len() - 16];
        let err = Model::from_bytes(truncated).unwrap_err();
        assert!(matches!(err, EngineError::UnexpectedEof { .. }));
    }

    #[test]
    fn model_rejects_bad_byte_order() {
        let mut header = sample_header(0, 0);
        header.bo = 0x0100; // big-endian
        let bytes = header.to_le_bytes();
        let err = Model::from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, EngineError::InvalidByteOrder(0x0100)));
    }

    #[test]
    fn vertex_color_preserved() {
        let v = BrVertex {
            position: Vec3::ZERO,
            uv: [FixedScalar::ZERO; 2],
            index: 42,
            red: 200,
            green: 100,
            blue: 50,
            normal: [0; 3],
        };
        let bytes = v.to_le_bytes();
        let v2 = BrVertex::from_le_bytes(&bytes);
        assert_eq!(v2.index, 42);
        assert_eq!(v2.red, 200);
        assert_eq!(v2.green, 100);
        assert_eq!(v2.blue, 50);
    }

    #[test]
    fn vertex_normal_fraction_range() {
        // br_fraction: i16 where 0x7FFF ≈ 1.0, 0x8001 ≈ -1.0
        let v = BrVertex {
            position: Vec3::ZERO,
            uv: [FixedScalar::ZERO; 2],
            index: 0,
            red: 0,
            green: 0,
            blue: 0,
            normal: [0x7FFF, -0x7FFF, 0],
        };
        let bytes = v.to_le_bytes();
        let v2 = BrVertex::from_le_bytes(&bytes);
        assert_eq!(v2.normal[0], 0x7FFF);
        assert_eq!(v2.normal[1], -0x7FFF);
    }
}
