//! Material type for the wgpu renderer.
//!
//! Maps BRender's `br_material` to a GPU-friendly struct.

use bytemuck::{Pod, Zeroable};

/// Material properties sent to the GPU as a uniform.
///
/// Mirrors BRender `br_material` key fields:
///   colour   → base_color
///   ka/kd/ks → ambient / diffuse / specular coefficients
///   power    → specular_power
///   opacity  → alpha
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuMaterial {
    /// RGBA base color (from `br_material.colour`).
    pub base_color: [f32; 4],
    /// Ambient coefficient  (BRender `ka`, 0.0–1.0).
    pub ambient: f32,
    /// Diffuse coefficient  (BRender `kd`, 0.0–1.0).
    pub diffuse: f32,
    /// Specular coefficient (BRender `ks`, 0.0–1.0).
    pub specular: f32,
    /// Specular power / shininess (BRender `power`).
    pub specular_power: f32,
}

impl Default for GpuMaterial {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.8, 1.0],
            ambient: 0.2,
            diffuse: 0.7,
            specular: 0.3,
            specular_power: 20.0,
        }
    }
}

/// CPU-side material with optional texture reference.
pub struct Material {
    pub gpu: GpuMaterial,
    /// If `true`, use per-vertex colors instead of `base_color`.
    pub prelit: bool,
    /// If `true`, render both sides of faces.
    pub two_sided: bool,
    // TODO Phase 3+: texture_id for TMAP lookup
}

impl Default for Material {
    fn default() -> Self {
        Self {
            gpu: GpuMaterial::default(),
            prelit: false,
            two_sided: false,
        }
    }
}
