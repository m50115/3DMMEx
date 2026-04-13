//! Lighting types for the wgpu renderer.
//!
//! BRender supports point, directional, and spot lights.
//! 3DMM backgrounds (BKGD) define one ambient + one directional light.

use bytemuck::{Pod, Zeroable};

/// Light uniform sent to the GPU.
///
/// Supports directional light (the primary type in 3DMM scenes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuLight {
    /// Normalized direction (world space, towards the light).
    pub direction: [f32; 3],
    pub _pad0: f32,
    /// Light color (RGB, 0.0–1.0).
    pub color: [f32; 3],
    /// Light intensity multiplier.
    pub intensity: f32,
    /// Ambient color (RGB, 0.0–1.0).
    pub ambient: [f32; 3],
    pub _pad1: f32,
}

impl Default for GpuLight {
    fn default() -> Self {
        Self {
            direction: [0.0, -1.0, -0.5],
            _pad0: 0.0,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            ambient: [0.15, 0.15, 0.15],
            _pad1: 0.0,
        }
    }
}
