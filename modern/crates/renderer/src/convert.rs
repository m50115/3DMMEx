//! BRender model, material, camera, light, and texture → GPU conversion.
//!
//! Converts engine domain types (fixed-point) to renderer types (f32).
//! This is the boundary where BRS 16.16 → f32 and br_fraction i16 → f32.

use engine::background::{Bmat34, BrCamera, BrLight, bra_to_radians};
use engine::events::OrientPayload;
use engine::fixedpoint::FixedScalar;
use engine::material::BrMaterial;
use engine::model::{BrVertex, Model};
use engine::tmap::{BrPixelType, BrTmap};
use engine::transform::Vec3;
use glam;

use crate::camera::Camera;
use crate::lighting::GpuLight;
use crate::material::{GpuMaterial, GpuTexture, Material};
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
        texture_idx: None,
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

/// Convert a parsed [`BrTmap`] to an RGBA8 pixel buffer for GPU upload.
///
/// Returns `(width, height, rgba_bytes)` where `rgba_bytes` has length
/// `width × height × 4`.
///
/// Pixel format handling:
/// - `INDEX_8`: 8-bit index treated as greyscale (R=G=B=index, A=255).
///   Full shade-table palette lookup is not implemented; greyscale is a
///   safe stand-in for pipeline testing.
/// - `RGB_555`: 5-5-5 packed → RGBA8 (each 5-bit channel expanded to 8-bit).
/// - `RGB_565`: 5-6-5 packed → RGBA8.
/// - `RGB_888`: 24-bit → RGBA8 (A=255).
/// - `RGBX_888` / `RGBA_8888`: 32-bit → RGBA8 (X ignored, A=255).
/// - Unknown formats: solid white (255,255,255,255) per pixel.
///
/// Row stride (`row_bytes`) is respected — only `width` pixels per row
/// are read, skipping any padding bytes.
pub fn tmap_to_rgba(tmap: &BrTmap) -> (u32, u32, Vec<u8>) {
    let width     = tmap.width.max(0) as usize;
    let height    = tmap.height.max(0) as usize;
    let row_bytes = tmap.row_bytes.max(0) as usize;
    let mut out   = Vec::with_capacity(width * height * 4);

    match tmap.pixel_type_parsed() {
        BrPixelType::Index8 => {
            for row in 0..height {
                let rs = row * row_bytes;
                for col in 0..width {
                    let idx = tmap.pixels[rs + col];
                    out.push(idx); out.push(idx); out.push(idx); out.push(255);
                }
            }
        }
        BrPixelType::Rgb555 => {
            for row in 0..height {
                let rs = row * row_bytes;
                for col in 0..width {
                    let lo = tmap.pixels[rs + col * 2];
                    let hi = tmap.pixels[rs + col * 2 + 1];
                    let p  = u16::from_le_bytes([lo, hi]);
                    let r = ((p >> 10) & 0x1F) as u8;
                    let g = ((p >>  5) & 0x1F) as u8;
                    let b = ( p        & 0x1F) as u8;
                    // Expand 5-bit → 8-bit: shift up and replicate high bits into low bits
                    out.push((r << 3) | (r >> 2));
                    out.push((g << 3) | (g >> 2));
                    out.push((b << 3) | (b >> 2));
                    out.push(255);
                }
            }
        }
        BrPixelType::Rgb565 => {
            for row in 0..height {
                let rs = row * row_bytes;
                for col in 0..width {
                    let lo = tmap.pixels[rs + col * 2];
                    let hi = tmap.pixels[rs + col * 2 + 1];
                    let p  = u16::from_le_bytes([lo, hi]);
                    let r = ((p >> 11) & 0x1F) as u8;
                    let g = ((p >>  5) & 0x3F) as u8;
                    let b = ( p        & 0x1F) as u8;
                    out.push((r << 3) | (r >> 2));
                    out.push((g << 2) | (g >> 4));
                    out.push((b << 3) | (b >> 2));
                    out.push(255);
                }
            }
        }
        BrPixelType::Rgb888 => {
            for row in 0..height {
                let rs = row * row_bytes;
                for col in 0..width {
                    let base = rs + col * 3;
                    out.push(tmap.pixels[base]);
                    out.push(tmap.pixels[base + 1]);
                    out.push(tmap.pixels[base + 2]);
                    out.push(255);
                }
            }
        }
        BrPixelType::RgbX888 | BrPixelType::Rgba8888 => {
            for row in 0..height {
                let rs = row * row_bytes;
                for col in 0..width {
                    let base = rs + col * 4;
                    out.push(tmap.pixels[base]);
                    out.push(tmap.pixels[base + 1]);
                    out.push(tmap.pixels[base + 2]);
                    out.push(255);
                }
            }
        }
        BrPixelType::Unknown(_) => {
            // Solid white — unrecognised format; at least we get correct geometry.
            for _ in 0..width * height {
                out.extend_from_slice(&[255, 255, 255, 255]);
            }
        }
    }

    (width as u32, height as u32, out)
}

/// Upload a parsed [`BrTmap`] to the GPU as a 2-D RGBA8 texture.
///
/// Internally calls [`tmap_to_rgba`] to produce the pixel buffer, then
/// creates a `wgpu::TextureFormat::Rgba8UnormSrgb` texture and writes the
/// data with `queue.write_texture`.
///
/// Returns a [`GpuTexture`] containing the texture, a default view, and a
/// bilinear clamp-to-edge sampler.
pub fn tmap_to_texture(
    tmap: &BrTmap,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> GpuTexture {
    let (width, height, rgba) = tmap_to_rgba(tmap);

    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tmap_texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("tmap_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    GpuTexture { texture, view, sampler }
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
    use engine::tmap::{BrTmap, BR_PMT_INDEX_8, BR_PMT_RGB_565};
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

    // ── tmap_to_rgba ─────────────────────────────────────────────────────────

    fn make_tmap(pixel_type: u8, width: i16, height: i16, row_bytes: i16, pixels: Vec<u8>) -> BrTmap {
        BrTmap { row_bytes, pixel_type, flags: 0, base_x: 0, base_y: 0,
                 width, height, origin_x: 0, origin_y: 0, pixels }
    }

    #[test]
    fn index8_greyscale_output() {
        // 2×1 INDEX_8 image: pixels [0, 255]
        let tmap = make_tmap(BR_PMT_INDEX_8, 2, 1, 2, vec![0x00, 0xFF]);
        let (w, h, rgba) = tmap_to_rgba(&tmap);
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        assert_eq!(rgba.len(), 8); // 2×1×4
        // First pixel: black (0,0,0,255)
        assert_eq!(&rgba[0..4], &[0, 0, 0, 255]);
        // Second pixel: white (255,255,255,255)
        assert_eq!(&rgba[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn index8_respects_row_stride() {
        // 2×2 INDEX_8 with cbRow=4 (2 active + 2 padding per row)
        let pixels = vec![
            0x10, 0x20, 0xAA, 0xAA, // row 0: [0x10, 0x20] + 2 pad bytes
            0x30, 0x40, 0xAA, 0xAA, // row 1: [0x30, 0x40] + 2 pad bytes
        ];
        let tmap = make_tmap(BR_PMT_INDEX_8, 2, 2, 4, pixels);
        let (w, h, rgba) = tmap_to_rgba(&tmap);
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(rgba.len(), 16);
        // Row 0 pixel 0: 0x10 greyscale
        assert_eq!(&rgba[0..4], &[0x10, 0x10, 0x10, 255]);
        // Row 1 pixel 0: 0x30 greyscale — padding bytes skipped
        assert_eq!(&rgba[8..12], &[0x30, 0x30, 0x30, 255]);
    }

    #[test]
    fn rgb565_expands_to_rgba() {
        // One pixel: pure red in RGB565 = 0b11111_000000_00000 = 0xF800
        let p = 0xF800u16.to_le_bytes();
        let tmap = make_tmap(BR_PMT_RGB_565, 1, 1, 2, p.to_vec());
        let (w, h, rgba) = tmap_to_rgba(&tmap);
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(rgba.len(), 4);
        // R=31 → (31<<3)|(31>>2) = 248|7 = 255; G=0; B=0; A=255
        assert_eq!(rgba[0], 255, "R");
        assert_eq!(rgba[1], 0,   "G");
        assert_eq!(rgba[2], 0,   "B");
        assert_eq!(rgba[3], 255, "A");
    }

    #[test]
    fn tmap_rgba_output_length() {
        // Any format: output must be width*height*4 bytes
        let tmap = make_tmap(BR_PMT_INDEX_8, 8, 8, 8, vec![0u8; 64]);
        let (w, h, rgba) = tmap_to_rgba(&tmap);
        assert_eq!(w, 8);
        assert_eq!(h, 8);
        assert_eq!(rgba.len(), 8 * 8 * 4);
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
