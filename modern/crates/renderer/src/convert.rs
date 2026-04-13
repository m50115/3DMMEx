//! BRender model, material, camera and light → GPU conversion.
//!
//! Converts engine domain types (fixed-point) to renderer types (f32).
//! This is the boundary where BRS 16.16 → f32 and br_fraction i16 → f32.

use engine::background::{Bmat34, BrCamera, BrLight, bra_to_radians};
use engine::events::OrientPayload;
use engine::fixedpoint::FixedScalar;
use engine::material::BrMaterial;
use engine::model::{BrVertex, Model};
use engine::transform::Vec3;
use glam;

use crate::camera::Camera;
use crate::lighting::GpuLight;
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

/// Convert a parsed [`BrCamera`] into a renderer [`Camera`].
///
/// - `a_fov` (BRA u16) → `fov_y` in radians
/// - `hither_z` / `yon_z` (BRS) → `near` / `far` as f32
/// - `bmat34` row 3 → camera position; row 2 → forward direction
/// - `aspect` must be supplied by the caller (viewport width / height)
pub fn camera_to_renderer(cam: &BrCamera, aspect: f32) -> Camera {
    let pos = cam.bmat34.translation_f32();
    let fwd = cam.bmat34.forward_f32();

    // BRender cameras look in +Z of their local frame (row 2 = forward).
    let position = glam::Vec3::new(pos[0], pos[1], pos[2]);
    let target   = glam::Vec3::new(pos[0] + fwd[0], pos[1] + fwd[1], pos[2] + fwd[2]);

    Camera {
        position,
        target,
        up: glam::Vec3::Y,
        fov_y: cam.fov_radians(),
        near: cam.hither_f32(),
        far:  cam.yon_f32(),
        aspect,
    }
}

/// Convert a parsed [`BrLight`] into a renderer [`GpuLight`].
///
/// - `bmat34` row 2 → light direction (forward z-axis of the light's frame)
/// - `r_intensity` (BRS) → `intensity` as f32
/// - Colour is always white (3DMM does not store per-light RGB on disk)
/// - Ambient is kept at the renderer default (0.15 grey)
pub fn light_to_renderer(lite: &BrLight) -> GpuLight {
    let dir = lite.bmat34.forward_f32();
    GpuLight {
        direction: dir,
        _pad0: 0.0,
        color: [1.0, 1.0, 1.0],
        intensity: lite.intensity_f32(),
        ambient: [0.15, 0.15, 0.15],
        _pad1: 0.0,
    }
}

/// Convert a BRender BMAT34 (4×3 affine matrix, row-major) to a glam `Mat4`.
///
/// BRender layout (4 rows × 3 cols):
///   Row 0 = X-axis (right), Row 1 = Y-axis (up),
///   Row 2 = Z-axis (forward), Row 3 = Translation.
///
/// glam `Mat4` is column-major; each BRender row becomes a glam column.
pub fn bmat34_to_mat4(mat: &Bmat34) -> glam::Mat4 {
    let f = |v: FixedScalar| v.0 as f32 / 65536.0;
    glam::Mat4::from_cols(
        glam::Vec4::new(f(mat.m[0][0]), f(mat.m[0][1]), f(mat.m[0][2]), 0.0),
        glam::Vec4::new(f(mat.m[1][0]), f(mat.m[1][1]), f(mat.m[1][2]), 0.0),
        glam::Vec4::new(f(mat.m[2][0]), f(mat.m[2][1]), f(mat.m[2][2]), 0.0),
        glam::Vec4::new(f(mat.m[3][0]), f(mat.m[3][1]), f(mat.m[3][2]), 1.0),
    )
}

/// Build a world-space translation matrix for an actor at a route point.
///
/// 3DMM world position = `route_point.position + actor.dxyz_full_rte`.
pub fn actor_translation_mat4(route_pos: &Vec3, dxyz: &Vec3) -> glam::Mat4 {
    let x = brs_to_f32(route_pos.x) + brs_to_f32(dxyz.x);
    let y = brs_to_f32(route_pos.y) + brs_to_f32(dxyz.y);
    let z = brs_to_f32(route_pos.z) + brs_to_f32(dxyz.z);
    glam::Mat4::from_translation(glam::Vec3::new(x, y, z))
}

/// Build a rotation matrix from `AEV_ORIENT` / `AEV_ROTATE` Euler angles (BRA).
///
/// 3DMM applies rotations in X→Y→Z order (pitch → yaw → roll).
pub fn orient_to_rotation_mat4(orient: &OrientPayload) -> glam::Mat4 {
    let pitch = bra_to_radians(orient.xa.0);
    let yaw   = bra_to_radians(orient.ya.0);
    let roll  = bra_to_radians(orient.za.0);
    glam::Mat4::from_euler(glam::EulerRot::XYZ, pitch, yaw, roll)
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
    use engine::background::Bmat34;
    use engine::events::OrientPayload;
    use engine::fixedpoint::{FixedAngle, FixedScalar};
    use engine::material::BrMaterial;
    use engine::model::{BrFaceFile, Bounds, ModelHeader, Model};
    use engine::transform::Vec3;

    // ── bmat34_to_mat4 ──────────────────────────────────────────────────────

    #[test]
    fn bmat34_identity_becomes_glam_identity() {
        let m = bmat34_to_mat4(&Bmat34::IDENTITY);
        // Diagonal = 1.0
        assert!((m.col(0).x - 1.0).abs() < 1e-6, "col0.x");
        assert!((m.col(1).y - 1.0).abs() < 1e-6, "col1.y");
        assert!((m.col(2).z - 1.0).abs() < 1e-6, "col2.z");
        assert!((m.col(3).w - 1.0).abs() < 1e-6, "col3.w");
        // Off-diagonal = 0.0
        assert!((m.col(0).y).abs() < 1e-6, "col0.y");
        assert!((m.col(0).z).abs() < 1e-6, "col0.z");
        assert!((m.col(3).x).abs() < 1e-6, "translation.x");
        assert!((m.col(3).y).abs() < 1e-6, "translation.y");
        assert!((m.col(3).z).abs() < 1e-6, "translation.z");
    }

    #[test]
    fn bmat34_translation_goes_to_col3() {
        let mut mat = Bmat34::IDENTITY;
        mat.m[3][0] = FixedScalar(0x0003_0000); // 3.0
        mat.m[3][1] = FixedScalar(0x0004_0000); // 4.0
        mat.m[3][2] = FixedScalar(0x0005_0000); // 5.0
        let m = bmat34_to_mat4(&mat);
        assert!((m.col(3).x - 3.0).abs() < 1e-5, "tx={}", m.col(3).x);
        assert!((m.col(3).y - 4.0).abs() < 1e-5, "ty={}", m.col(3).y);
        assert!((m.col(3).z - 5.0).abs() < 1e-5, "tz={}", m.col(3).z);
        assert!((m.col(3).w - 1.0).abs() < 1e-6, "tw={}", m.col(3).w);
        // Rotation unchanged (identity)
        assert!((m.col(0).x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bmat34_rows_map_to_columns() {
        // Row 0 (right) → col 0; Row 1 (up) → col 1; Row 2 (fwd) → col 2
        let mut mat = Bmat34::IDENTITY;
        // Set row 1 (up) to a non-trivial vector (0.0, 2.0, 0.0)
        mat.m[1][1] = FixedScalar(0x0002_0000); // 2.0
        let m = bmat34_to_mat4(&mat);
        assert!((m.col(1).y - 2.0).abs() < 1e-5, "up.y={}", m.col(1).y);
        assert!((m.col(1).x).abs() < 1e-6);
        assert!((m.col(1).z).abs() < 1e-6);
    }

    // ── actor_translation_mat4 ───────────────────────────────────────────────

    #[test]
    fn actor_translation_combines_route_and_offset() {
        let pos = Vec3 {
            x: FixedScalar(0x0001_0000), // 1.0
            y: FixedScalar(0x0002_0000), // 2.0
            z: FixedScalar(0x0000_0000),
        };
        let dxyz = Vec3 {
            x: FixedScalar(0x0000_8000), // 0.5
            y: FixedScalar(0x0000_0000),
            z: FixedScalar(-0x0001_0000), // -1.0
        };
        let m = actor_translation_mat4(&pos, &dxyz);
        assert!((m.col(3).x - 1.5).abs() < 1e-5, "x={}", m.col(3).x);
        assert!((m.col(3).y - 2.0).abs() < 1e-5, "y={}", m.col(3).y);
        assert!((m.col(3).z - (-1.0)).abs() < 1e-5, "z={}", m.col(3).z);
        assert!((m.col(3).w - 1.0).abs() < 1e-6);
    }

    #[test]
    fn actor_translation_zero_offset() {
        let pos = Vec3 {
            x: FixedScalar(0x0007_0000), // 7.0
            y: FixedScalar(0x0000_0000),
            z: FixedScalar(0x0003_0000), // 3.0
        };
        let m = actor_translation_mat4(&pos, &Vec3::ZERO);
        assert!((m.col(3).x - 7.0).abs() < 1e-5);
        assert!((m.col(3).y).abs() < 1e-5);
        assert!((m.col(3).z - 3.0).abs() < 1e-5);
    }

    // ── orient_to_rotation_mat4 ──────────────────────────────────────────────

    #[test]
    fn orient_zero_angles_is_identity() {
        let orient = OrientPayload {
            xa: FixedAngle(0),
            ya: FixedAngle(0),
            za: FixedAngle(0),
        };
        let m = orient_to_rotation_mat4(&orient);
        assert!((m.col(0).x - 1.0).abs() < 1e-5);
        assert!((m.col(1).y - 1.0).abs() < 1e-5);
        assert!((m.col(2).z - 1.0).abs() < 1e-5);
        assert!((m.col(0).y).abs() < 1e-5);
    }

    #[test]
    fn orient_quarter_turn_y() {
        // 90° yaw (ya = 0x4000 = π/2)
        let orient = OrientPayload {
            xa: FixedAngle(0),
            ya: FixedAngle(0x4000),
            za: FixedAngle(0),
        };
        let m = orient_to_rotation_mat4(&orient);
        // After 90° Y rotation: R_y(90°): col0 = [0,0,-1,0], col2 = [1,0,0,0]
        assert!((m.col(0).z - (-1.0)).abs() < 1e-5, "col0.z={}", m.col(0).z);
        assert!((m.col(2).x - 1.0).abs() < 1e-5, "col2.x={}", m.col(2).x);
        assert!((m.col(1).y - 1.0).abs() < 1e-5, "col1.y={}", m.col(1).y);
    }

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
