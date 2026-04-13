//! # renderer
//!
//! wgpu-based 3D renderer reproducing BRender output.
//!
//! Module hierarchy:
//!   vertex / material / camera / lighting  (data types)
//!   → world                                (scene graph)
//!   → pipeline                             (GPU state)
//!   → window                               (winit event loop)

pub mod camera;
pub mod convert;
pub mod lighting;
pub mod material;
pub mod pipeline;
pub mod vertex;
pub mod window;
pub mod world;
