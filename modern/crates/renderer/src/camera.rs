//! Camera system for the wgpu renderer.
//!
//! Mirrors BRender `br_camera` (perspective FOV mode).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// Camera uniform sent to the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuCamera {
    /// Combined view-projection matrix (column-major).
    pub view_proj: [[f32; 4]; 4],
    /// Camera world-space position (for specular).
    pub eye_pos: [f32; 3],
    pub _pad: f32,
}

/// CPU-side camera built from BRender `br_camera` fields.
pub struct Camera {
    /// World-space position.
    pub position: Vec3,
    /// Look-at target.
    pub target: Vec3,
    /// Up vector.
    pub up: Vec3,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Near clipping plane (BRender `hither_z`).
    pub near: f32,
    /// Far clipping plane (BRender `yon_z`).
    pub far: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 2.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: std::f32::consts::FRAC_PI_4, // 45°
            near: 0.1,
            far: 100.0,
            aspect: 16.0 / 9.0,
        }
    }

    /// Build the view matrix (world → camera).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Build the projection matrix.
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far)
    }

    /// Build the GPU uniform.
    pub fn to_gpu(&self) -> GpuCamera {
        let view_proj = self.projection_matrix() * self.view_matrix();
        GpuCamera {
            view_proj: view_proj.to_cols_array_2d(),
            eye_pos: self.position.to_array(),
            _pad: 0.0,
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}
