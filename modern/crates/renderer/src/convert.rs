//! BRender model and material → GPU conversion.
//!
//! Converts engine domain types (fixed-point) to renderer types (f32).
//! This is the boundary where BRS 16.16 → f32 and br_fraction i16 → f32.

use engine::fixedpoint::FixedScalar;
use engine::material::BrMaterial;
use engine::model::{BrVertex, Model};

use crate::material::{GpuMaterial, Material};
use crate::vertex::{GpuVertex, Mesh};

/// Convert a BRender fixed-point scalar to f32.
fn brs_to_f32(s: FixedScalar) -> f32 {
    s.0 as f32 / 65536.0
}

/// Convert a BRender fraction (i16) to f32. Range: [-1.0, ~1.0).
fn fraction_to_f32(f: i16) -> f32 {
    f as f32 / 32768.0
}

/// Convert a BRender vertex to a GPU vertex.
fn vertex_to_gpu(v: &BrVertex) -> GpuVertex {
    GpuVertex {
        position: [
            brs_to_f32(v.position.x),
            brs_to_f32(v.position.y),
            brs_to_f32(v.position.z),
        ],
        normal: [
            fraction_to_f32(v.normal[0]),
            fraction_to_f32(v.normal[1]),
            fraction_to_f32(v.normal[2]),
        ],
        uv: [
            brs_to_f32(v.uv[0]),
            brs_to_f32(v.uv[1]),
        ],
        color: [
            v.red as f32 / 255.0,
            v.green as f32 / 255.0,
            v.blue as f32 / 255.0,
            1.0,
        ],
    }
}

/// Convert a parsed [`BrMaterial`] into a renderer [`Material`].
///
/// Conversions applied:
/// - `colour` (0x00RRGGBB) → `base_color` as f32 per channel / 255.0; alpha = 1.0
/// - `ka/kd/ks` (u16 br_ufraction) → f32 / 65535.0
/// - `power` (BRS 16.16 i32) → f32 / 65536.0
pub fn material_to_gpu(mat: &BrMaterial) -> Material {
    Material {
        gpu: GpuMaterial {
            base_color: [
                mat.red()   as f32 / 255.0,
                mat.green() as f32 / 255.0,
                mat.blue()  as f32 / 255.0,
                1.0, // opacity always 1.0 (3DMM Socrates always sets kbOpaque=0xFF)
            ],
            ambient:       mat.ka as f32 / 65535.0,
            diffuse:       mat.kd as f32 / 65535.0,
            specular:      mat.ks as f32 / 65535.0,
            specular_power: mat.power.0 as f32 / 65536.0,
        },
        prelit: false,    // MTRLF has no flags field; use default
        two_sided: false,
    }
}

/// Convert a parsed BRender [`Model`] into a renderable [`Mesh`].
///
/// Vertices are converted from fixed-point to f32.
/// Face vertex indices are widened from u16 to u32.
pub fn model_to_mesh(model: &Model) -> Mesh {
    let vertices: Vec<GpuVertex> = model.vertices.iter().map(vertex_to_gpu).collect();

    let indices: Vec<u32> = model
        .faces
        .iter()
        .flat_map(|f| f.vertices.iter().map(|&idx| idx as u32))
        .collect();

    let radius = brs_to_f32(model.header.radius);

    Mesh::new(vertices, indices, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::fixedpoint::FixedScalar;
    use engine::material::BrMaterial;
    use engine::model::{BrFaceFile, Bounds, ModelHeader, Model};
    use engine::transform::Vec3;

    fn make_triangle_model() -> Model {
        let v0 = BrVertex {
            position: Vec3 {
                x: FixedScalar(0),             // 0.0
                y: FixedScalar(0x0001_0000),   // 1.0
                z: FixedScalar(0),
            },
            uv: [FixedScalar(0x0000_8000), FixedScalar(0)], // (0.5, 0.0)
            index: 0,
            red: 255, green: 0, blue: 0,
            normal: [0, 0, 0x7FFF], // (0, 0, ~1)
        };
        let v1 = BrVertex {
            position: Vec3 {
                x: FixedScalar(-0x0001_0000),  // -1.0
                y: FixedScalar(-0x0001_0000),  // -1.0
                z: FixedScalar(0),
            },
            uv: [FixedScalar(0), FixedScalar(0x0001_0000)], // (0, 1)
            index: 0,
            red: 0, green: 255, blue: 0,
            normal: [0, 0, 0x7FFF],
        };
        let v2 = BrVertex {
            position: Vec3 {
                x: FixedScalar(0x0001_0000),   // 1.0
                y: FixedScalar(-0x0001_0000),  // -1.0
                z: FixedScalar(0),
            },
            uv: [FixedScalar(0x0001_0000), FixedScalar(0x0001_0000)], // (1, 1)
            index: 0,
            red: 0, green: 0, blue: 255,
            normal: [0, 0, 0x7FFF],
        };

        let face = BrFaceFile {
            vertices: [0, 1, 2],
            edges: [0, 1, 2],
            material: 0,
            smoothing: 1,
            flags: 0,
            normal: [0, 0, 0x7FFF],
            d: FixedScalar(0),
        };

        Model {
            header: ModelHeader {
                bo: 0x0001,
                osk: 0x7769,
                vertex_count: 3,
                face_count: 1,
                radius: FixedScalar(0x0001_6A0A), // ~1.414
                bounds: Bounds {
                    min: Vec3 {
                        x: FixedScalar(-0x0001_0000),
                        y: FixedScalar(-0x0001_0000),
                        z: FixedScalar(0),
                    },
                    max: Vec3 {
                        x: FixedScalar(0x0001_0000),
                        y: FixedScalar(0x0001_0000),
                        z: FixedScalar(0),
                    },
                },
                pivot: Vec3::ZERO,
            },
            vertices: vec![v0, v1, v2],
            faces: vec![face],
        }
    }

    fn sample_brmaterial() -> BrMaterial {
        BrMaterial {
            colour: 0x00_FF_80_40, // R=255, G=128, B=64
            ka: 6553,              // ≈0.10
            kd: 39321,             // ≈0.60
            ks: 39321,             // ≈0.60
            index_base: 0,
            index_range: 0,
            power: FixedScalar(0x0032_0000), // 50.0
        }
    }

    #[test]
    fn material_base_color_from_colour() {
        let mat = material_to_gpu(&sample_brmaterial());
        assert!((mat.gpu.base_color[0] - 1.0).abs() < 0.005, "R={}", mat.gpu.base_color[0]);
        assert!((mat.gpu.base_color[1] - 0.502).abs() < 0.005, "G={}", mat.gpu.base_color[1]);
        assert!((mat.gpu.base_color[2] - 0.251).abs() < 0.005, "B={}", mat.gpu.base_color[2]);
        assert_eq!(mat.gpu.base_color[3], 1.0);
    }

    #[test]
    fn material_coefficients_from_fractions() {
        let mat = material_to_gpu(&sample_brmaterial());
        assert!((mat.gpu.ambient  - 0.10).abs() < 0.002, "ambient={}", mat.gpu.ambient);
        assert!((mat.gpu.diffuse  - 0.60).abs() < 0.002, "diffuse={}", mat.gpu.diffuse);
        assert!((mat.gpu.specular - 0.60).abs() < 0.002, "specular={}", mat.gpu.specular);
    }

    #[test]
    fn material_specular_power_from_brs() {
        let mat = material_to_gpu(&sample_brmaterial());
        // 0x0032_0000 / 65536 = 50.0
        assert!((mat.gpu.specular_power - 50.0).abs() < 0.01, "power={}", mat.gpu.specular_power);
    }

    #[test]
    fn material_alpha_always_opaque() {
        let mat = material_to_gpu(&sample_brmaterial());
        assert_eq!(mat.gpu.base_color[3], 1.0);
    }

    #[test]
    fn material_default_flags() {
        let mat = material_to_gpu(&sample_brmaterial());
        assert!(!mat.prelit);
        assert!(!mat.two_sided);
    }

    #[test]
    fn material_black_colour() {
        let black = BrMaterial {
            colour: 0x00_00_00_00,
            ka: 0, kd: 0, ks: 0,
            index_base: 0, index_range: 0,
            power: FixedScalar(0),
        };
        let mat = material_to_gpu(&black);
        assert_eq!(mat.gpu.base_color[0], 0.0);
        assert_eq!(mat.gpu.base_color[1], 0.0);
        assert_eq!(mat.gpu.base_color[2], 0.0);
        assert_eq!(mat.gpu.base_color[3], 1.0);
        assert_eq!(mat.gpu.ambient, 0.0);
        assert_eq!(mat.gpu.specular_power, 0.0);
    }

    #[test]
    fn brs_conversion() {
        assert_eq!(brs_to_f32(FixedScalar(0x0001_0000)), 1.0);
        assert_eq!(brs_to_f32(FixedScalar(0x0000_8000)), 0.5);
        assert_eq!(brs_to_f32(FixedScalar(0)), 0.0);
        assert!((brs_to_f32(FixedScalar(-0x0001_0000)) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn fraction_conversion() {
        // 0x7FFF / 32768 ≈ 0.99997
        let f = fraction_to_f32(0x7FFF);
        assert!(f > 0.999 && f < 1.001);

        // -0x7FFF / 32768 ≈ -0.99997
        let f2 = fraction_to_f32(-0x7FFF);
        assert!(f2 > -1.001 && f2 < -0.999);

        assert_eq!(fraction_to_f32(0), 0.0);
    }

    #[test]
    fn triangle_mesh_geometry() {
        let model = make_triangle_model();
        let mesh = model_to_mesh(&model);

        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.index_count(), 3);

        // Vertex 0: position (0, 1, 0)
        assert_eq!(mesh.vertices[0].position[0], 0.0);
        assert_eq!(mesh.vertices[0].position[1], 1.0);
        assert_eq!(mesh.vertices[0].position[2], 0.0);

        // Vertex 1: position (-1, -1, 0)
        assert!((mesh.vertices[1].position[0] - (-1.0)).abs() < 1e-6);
        assert!((mesh.vertices[1].position[1] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn triangle_mesh_colors() {
        let model = make_triangle_model();
        let mesh = model_to_mesh(&model);

        // V0: red (255, 0, 0) → (1.0, 0.0, 0.0, 1.0)
        assert!((mesh.vertices[0].color[0] - 1.0).abs() < 1e-3);
        assert_eq!(mesh.vertices[0].color[1], 0.0);
        assert_eq!(mesh.vertices[0].color[2], 0.0);
        assert_eq!(mesh.vertices[0].color[3], 1.0);

        // V1: green
        assert_eq!(mesh.vertices[1].color[0], 0.0);
        assert!((mesh.vertices[1].color[1] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn triangle_mesh_normals() {
        let model = make_triangle_model();
        let mesh = model_to_mesh(&model);

        // All normals point in +Z: (0, 0, ~1)
        for v in &mesh.vertices {
            assert!(v.normal[2] > 0.999);
            assert!(v.normal[0].abs() < 1e-6);
            assert!(v.normal[1].abs() < 1e-6);
        }
    }

    #[test]
    fn triangle_mesh_uvs() {
        let model = make_triangle_model();
        let mesh = model_to_mesh(&model);

        // V0: uv (0.5, 0.0)
        assert_eq!(mesh.vertices[0].uv[0], 0.5);
        assert_eq!(mesh.vertices[0].uv[1], 0.0);
    }

    #[test]
    fn triangle_mesh_indices() {
        let model = make_triangle_model();
        let mesh = model_to_mesh(&model);

        assert_eq!(mesh.indices, vec![0, 1, 2]);
    }

    #[test]
    fn mesh_radius() {
        let model = make_triangle_model();
        let mesh = model_to_mesh(&model);
        // 0x0001_6A0A / 65536 ≈ 1.414
        assert!((mesh.radius - 1.414).abs() < 0.01);
    }

    #[test]
    fn multi_face_indices() {
        let mut model = make_triangle_model();
        // Add a second face using vertices 2, 1, 0
        let face2 = BrFaceFile {
            vertices: [2, 1, 0],
            edges: [0, 1, 2],
            material: 0,
            smoothing: 1,
            flags: 0,
            normal: [0, 0, -0x7FFF],
            d: FixedScalar(0),
        };
        model.faces.push(face2);
        model.header.face_count = 2;

        let mesh = model_to_mesh(&model);
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.indices, vec![0, 1, 2, 2, 1, 0]);
    }
}
