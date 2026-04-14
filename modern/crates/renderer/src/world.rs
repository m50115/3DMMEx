//! Render world — the scene graph submitted for rendering.
//!
//! Equivalent to BRender's BWLD: holds meshes, materials, transforms,
//! camera, and lights for one frame.

use glam::Mat4;

use crate::camera::Camera;
use crate::lighting::GpuLight;
use crate::material::{GpuTexture, Material};
use crate::vertex::Mesh;

/// A positioned instance of a mesh in the world.
pub struct MeshInstance {
    /// Index into `RenderWorld::meshes`.
    pub mesh_idx: usize,
    /// Index into `RenderWorld::materials`.
    pub material_idx: usize,
    /// Model → world transform (from BRender BMAT34).
    pub transform: Mat4,
}

/// Everything needed to render one frame.
///
/// Populated from chunky-format chunks + engine structs,
/// then consumed by the GPU pipeline each frame.
pub struct RenderWorld {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    /// Uploaded TMAP textures. `Material::texture_idx` indexes into this.
    pub textures: Vec<GpuTexture>,
    pub instances: Vec<MeshInstance>,
    pub camera: Camera,
    pub light: GpuLight,
    /// Background clear color (from BKGD).
    pub clear_color: wgpu::Color,
}

impl RenderWorld {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            instances: Vec::new(),
            camera: Camera::default(),
            light: GpuLight::default(),
            clear_color: wgpu::Color {
                r: 0.1,
                g: 0.1,
                b: 0.15,
                a: 1.0,
            },
        }
    }
}

impl Default for RenderWorld {
    fn default() -> Self {
        Self::new()
    }
}
