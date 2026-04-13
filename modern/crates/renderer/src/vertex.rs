//! Vertex and mesh types for the wgpu renderer.
//!
//! These mirror BRender's `br_vertex` / `br_face` / `br_model` but use
//! f32 for GPU compatibility.  Fixed-point → float conversion happens
//! at the boundary (see `convert` module).

use bytemuck::{Pod, Zeroable};

// ---------------------------------------------------------------------------
// GPU vertex — matches the wgpu vertex buffer layout
// ---------------------------------------------------------------------------

/// Per-vertex data sent to the GPU.
///
/// Corresponds to BRender's `br_vertex` (32 bytes on disk) but
/// repacked for a modern pipeline:
///   position  : vec3<f32>
///   normal    : vec3<f32>
///   uv        : vec2<f32>
///   color     : vec4<f32>   (prelit vertex color, or white if unlit)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl GpuVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3,  // position
        1 => Float32x3,  // normal
        2 => Float32x2,  // uv
        3 => Float32x4,  // color
    ];

    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// ---------------------------------------------------------------------------
// Mesh — a collection of vertices + triangle indices
// ---------------------------------------------------------------------------

/// A renderable mesh (CPU side).
///
/// Built from BRender `br_model` data: vertex array + face array.
/// After construction, upload to GPU buffers for drawing.
pub struct Mesh {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    /// Bounding sphere radius (from BRender `br_model.radius`).
    pub radius: f32,
}

impl Mesh {
    pub fn new(vertices: Vec<GpuVertex>, indices: Vec<u32>, radius: f32) -> Self {
        Self { vertices, indices, radius }
    }

    /// Number of indices (= number of triangles × 3).
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}
