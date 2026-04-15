//! Headless (offscreen) GPU renderer.
//!
//! Provides a `HeadlessRenderer` that renders an `engine::Model` to raw RGBA8
//! pixels without a window. Used by the Tauri backend to produce viewport
//! frames as PNG images.
//!
//! Returns `None` / gracefully skips when no GPU adapter is available (CI, no GPU).

use std::collections::HashMap;

use engine::model::Model;
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::convert::model_to_mesh;
use crate::lighting::GpuLight;
use crate::material::{GpuMaterial, GpuTexture};
use crate::pipeline::{GpuModelUniform, RenderPipeline};

/// Per-model GPU resources cached by `(ctg, cno)` key.
struct CachedMesh {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    /// Local-space AABB — used to compute world-space bbox without re-iterating vertices.
    local_min: [f32; 3],
    local_max: [f32; 3],
}

/// Headless GPU context for offscreen rendering.
pub struct HeadlessRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Render pipeline — created once, reused across all render calls.
    pipeline: RenderPipeline,
    /// 1×1 white fallback texture — created once, shared across draw calls.
    fallback_texture: GpuTexture,
    /// Vertex/index buffers keyed by `(ctg, cno)` of the source BMDL chunk.
    /// Populated lazily on first scene render; invalidated on file change.
    mesh_cache: HashMap<(u32, u32), CachedMesh>,
}

impl HeadlessRenderer {
    const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    // ── Async core ────────────────────────────────────────────────────────────

    /// Async GPU init. Works on native and WASM (WebGPU).
    /// Returns `None` if no adapter is available (no GPU, CI, browser without WebGPU).
    pub async fn new_async() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("headless_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .ok()?;
        let pipeline = RenderPipeline::new(&device, Self::DEFAULT_FORMAT, 640, 480);
        let fallback_texture = Self::white_texture_1x1(&device, &queue);
        Some(Self { device, queue, pipeline, fallback_texture, mesh_cache: HashMap::new() })
    }

    /// Async render of one or more models to RGBA8 pixels.
    ///
    /// Each entry is `(model, world_transform)`. The camera auto-fits the
    /// combined world-space bounding box of all models.
    /// Returns raw RGBA8 pixel data, or an empty Vec if no renderable geometry.
    pub async fn render_models_async(
        &self,
        models: &[(&Model, Mat4)],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        // Filter to renderable models only.
        let renderable: Vec<(&Model, Mat4)> = models
            .iter()
            .filter(|(m, _)| m.has_valid_faces() && !m.vertices.is_empty())
            .map(|(m, t)| (*m, *t))
            .collect();
        if renderable.is_empty() {
            return Vec::new();
        }

        let device = &self.device;
        let queue = &self.queue;
        let render_format = Self::DEFAULT_FORMAT;
        let rp = &self.pipeline;

        // ── meshes + combined world-space bounding box ────────────────────────
        let mesh_transforms: Vec<_> =
            renderable.iter().map(|(m, t)| (model_to_mesh(m), *t)).collect();

        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
        for (mesh, transform) in &mesh_transforms {
            for v in &mesh.vertices {
                let wp = transform.transform_point3(glam::Vec3::from_array(v.position));
                min_x = min_x.min(wp.x);
                max_x = max_x.max(wp.x);
                min_y = min_y.min(wp.y);
                max_y = max_y.max(wp.y);
                min_z = min_z.min(wp.z);
                max_z = max_z.max(wp.z);
            }
        }
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        let cz = (min_z + max_z) * 0.5;
        let half_diag = ((max_x - min_x).powi(2)
            + (max_y - min_y).powi(2)
            + (max_z - min_z).powi(2))
        .sqrt()
            * 0.5;
        let fit_radius = half_diag.max(0.001);

        let mut camera = Camera::new();
        camera.aspect = width as f32 / height as f32;
        camera.target = glam::Vec3::new(cx, cy, cz);
        camera.position = glam::Vec3::new(cx, cy, max_z + fit_radius * 2.0);
        camera.near = fit_radius * 0.01;
        camera.far = fit_radius * 20.0;

        queue.write_buffer(&rp.camera_buf, 0, bytemuck::bytes_of(&camera.to_gpu()));
        queue.write_buffer(&rp.light_buf, 0, bytemuck::bytes_of(&GpuLight::default()));

        let fallback = &self.fallback_texture;

        // ── per-model GPU buffers + bind groups ───────────────────────────────
        // Use tuples instead of a named struct to avoid lifetime-param issues in async fn.
        // (vertex_buf, index_buf, instance_bg, index_count)
        let draw_calls: Vec<(wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup, u32)> =
            mesh_transforms.iter().enumerate().map(|(i, (mesh, transform))| {
                let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("vbuf_{i}")),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("ibuf_{i}")),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                let model_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("model_uniform_{i}")),
                    contents: bytemuck::bytes_of(&GpuModelUniform::from_transform(transform)),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let material_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("material_uniform_{i}")),
                    contents: bytemuck::bytes_of(&GpuMaterial::default()),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let instance_bg = rp.create_instance_bind_group(
                    device,
                    &model_buf,
                    &material_buf,
                    &fallback.view,
                    &fallback.sampler,
                );
                let index_count = mesh.index_count();
                (vertex_buf, index_buf, instance_bg, index_count)
            })
            .collect();

        // ── offscreen targets ─────────────────────────────────────────────────
        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // ── render pass ───────────────────────────────────────────────────────
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&rp.pipeline);
            rpass.set_bind_group(0, &rp.frame_bind_group, &[]);
            for (vbuf, ibuf, bg, index_count) in &draw_calls {
                rpass.set_bind_group(1, bg, &[]);
                rpass.set_vertex_buffer(0, vbuf.slice(..));
                rpass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..*index_count, 0, 0..1);
            }
        }

        // ── readback ─────────────────────────────────────────────────────────
        // bytes_per_row must be a multiple of 256 (wgpu requirement)
        let bytes_per_row = (width * 4).next_multiple_of(256);
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));

        // Drop GPU resources no longer needed before the await point.
        drop(draw_calls);
        drop(color_tex);
        drop(depth_tex);

        // B4 fix: oneshot instead of mpsc so the callback is WASM-compatible.
        // B3 fix: device.poll only on native — WebGPU browser backend drives completion.
        let (tx, rx) = futures_channel::oneshot::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        #[cfg(not(target_arch = "wasm32"))]
        device.poll(wgpu::Maintain::Wait);
        match rx.await {
            Ok(Ok(())) => {}
            _ => return Vec::new(),
        }

        let mapped = staging.slice(..).get_mapped_range();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let row_start = (row * bytes_per_row) as usize;
            let row_end = row_start + (width * 4) as usize;
            rgba.extend_from_slice(&mapped[row_start..row_end]);
        }
        rgba
    }

    /// Async render of a scene frame using cached GPU buffers.
    ///
    /// Each entry is `((ctg, cno), world_transform)`. Entries whose key is not
    /// in the cache are silently skipped — call [`ensure_mesh`] first.
    /// Returns raw RGBA8 pixels or an empty Vec if nothing is renderable.
    pub async fn render_scene_async(
        &self,
        models: &[((u32, u32), Mat4)],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        // Collect only cached entries.
        let entries: Vec<((u32, u32), Mat4)> = models
            .iter()
            .filter(|(key, _)| self.mesh_cache.contains_key(key))
            .map(|(key, t)| (*key, *t))
            .collect();
        if entries.is_empty() {
            return Vec::new();
        }

        let device = &self.device;
        let queue = &self.queue;
        let rp = &self.pipeline;
        let render_format = Self::DEFAULT_FORMAT;
        let fallback = &self.fallback_texture;

        // ── world-space bbox from local AABB × 8 corners ─────────────────────
        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
        for (key, transform) in &entries {
            let mesh = &self.mesh_cache[key];
            let [lx0, ly0, lz0] = mesh.local_min;
            let [lx1, ly1, lz1] = mesh.local_max;
            for &cx in &[lx0, lx1] {
                for &cy in &[ly0, ly1] {
                    for &cz in &[lz0, lz1] {
                        let wp = transform.transform_point3(glam::Vec3::new(cx, cy, cz));
                        min_x = min_x.min(wp.x);
                        max_x = max_x.max(wp.x);
                        min_y = min_y.min(wp.y);
                        max_y = max_y.max(wp.y);
                        min_z = min_z.min(wp.z);
                        max_z = max_z.max(wp.z);
                    }
                }
            }
        }
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        let cz = (min_z + max_z) * 0.5;
        let half_diag = ((max_x - min_x).powi(2)
            + (max_y - min_y).powi(2)
            + (max_z - min_z).powi(2))
        .sqrt()
            * 0.5;
        let fit_radius = half_diag.max(0.001);

        let mut camera = Camera::new();
        camera.aspect = width as f32 / height as f32;
        camera.target = glam::Vec3::new(cx, cy, cz);
        camera.position = glam::Vec3::new(cx, cy, max_z + fit_radius * 2.0);
        camera.near = fit_radius * 0.01;
        camera.far = fit_radius * 20.0;

        queue.write_buffer(&rp.camera_buf, 0, bytemuck::bytes_of(&camera.to_gpu()));
        queue.write_buffer(&rp.light_buf, 0, bytemuck::bytes_of(&GpuLight::default()));

        // ── per-frame draw calls: new model uniform, cached vertex/index bufs ─
        // Tuples (instance_bg, index_count, vertex_buf_ptr, index_buf_ptr) —
        // we keep the cached buffer *references* only until after the render pass.
        let draw_calls: Vec<(wgpu::BindGroup, u32, (u32, u32))> = entries
            .iter()
            .enumerate()
            .map(|(i, (key, transform))| {
                let mesh = &self.mesh_cache[key];
                let model_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("model_uniform_{i}")),
                    contents: bytemuck::bytes_of(&GpuModelUniform::from_transform(transform)),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let material_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("material_uniform_{i}")),
                    contents: bytemuck::bytes_of(&GpuMaterial::default()),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let instance_bg = rp.create_instance_bind_group(
                    device,
                    &model_buf,
                    &material_buf,
                    &fallback.view,
                    &fallback.sampler,
                );
                (instance_bg, mesh.index_count, *key)
            })
            .collect();

        // ── offscreen targets ─────────────────────────────────────────────────
        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // ── render pass ───────────────────────────────────────────────────────
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&rp.pipeline);
            rpass.set_bind_group(0, &rp.frame_bind_group, &[]);
            for (bg, index_count, key) in &draw_calls {
                let mesh = &self.mesh_cache[key];
                rpass.set_bind_group(1, bg, &[]);
                rpass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                rpass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..*index_count, 0, 0..1);
            }
        }

        // ── readback ─────────────────────────────────────────────────────────
        let bytes_per_row = (width * 4).next_multiple_of(256);
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));

        // Drop GPU resources no longer needed before the await point.
        drop(draw_calls);
        drop(color_tex);
        drop(depth_tex);

        // B4 fix: oneshot instead of mpsc so the callback is WASM-compatible.
        // B3 fix: device.poll only on native — WebGPU browser backend drives completion.
        let (tx, rx) = futures_channel::oneshot::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        #[cfg(not(target_arch = "wasm32"))]
        device.poll(wgpu::Maintain::Wait);
        match rx.await {
            Ok(Ok(())) => {}
            _ => return Vec::new(),
        }

        let mapped = staging.slice(..).get_mapped_range();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let row_start = (row * bytes_per_row) as usize;
            rgba.extend_from_slice(
                &mapped[row_start..row_start + (width * 4) as usize],
            );
        }
        rgba
    }

    // ── Native sync wrappers (preserved API — call sites unchanged) ───────────

    /// Try to acquire a GPU adapter and create the headless renderer.
    /// Returns `None` if no adapter is available (no GPU, CI environment, etc.).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn try_new() -> Option<Self> {
        pollster::block_on(Self::new_async())
    }

    /// Render a single `model` to an offscreen `width × height` texture.
    ///
    /// Thin wrapper around [`render_models`] with an identity transform.
    /// Returns an empty `Vec` if the model has no renderable faces.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_model(&self, model: &Model, width: u32, height: u32) -> Vec<u8> {
        self.render_models(&[(model, Mat4::IDENTITY)], width, height)
    }

    /// Render one or more models into a single offscreen `width × height` frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_models(&self, models: &[(&Model, Mat4)], width: u32, height: u32) -> Vec<u8> {
        pollster::block_on(self.render_models_async(models, width, height))
    }

    /// Render a scene frame using cached GPU buffers (sync wrapper).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_scene(&self, models: &[((u32, u32), Mat4)], width: u32, height: u32) -> Vec<u8> {
        pollster::block_on(self.render_scene_async(models, width, height))
    }

    // ── Mesh cache API ────────────────────────────────────────────────────────

    /// Return true if the GPU buffers for `key` are already uploaded.
    pub fn has_mesh(&self, key: (u32, u32)) -> bool {
        self.mesh_cache.contains_key(&key)
    }

    /// Upload vertex/index buffers for `model` under `key` if not already cached.
    /// No-op if `key` is already present or model has no renderable geometry.
    pub fn ensure_mesh(&mut self, key: (u32, u32), model: &Model) {
        if self.mesh_cache.contains_key(&key) {
            return;
        }
        if !model.has_valid_faces() || model.vertices.is_empty() {
            return;
        }
        let mesh = model_to_mesh(model);
        if mesh.vertices.is_empty() {
            return;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut min_z = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        let mut max_z = f32::MIN;
        for v in &mesh.vertices {
            min_x = min_x.min(v.position[0]);
            max_x = max_x.max(v.position[0]);
            min_y = min_y.min(v.position[1]);
            max_y = max_y.max(v.position[1]);
            min_z = min_z.min(v.position[2]);
            max_z = max_z.max(v.position[2]);
        }

        let vertex_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vbuf_cached"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibuf_cached"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.mesh_cache.insert(
            key,
            CachedMesh {
                vertex_buf,
                index_buf,
                index_count: mesh.index_count(),
                local_min: [min_x, min_y, min_z],
                local_max: [max_x, max_y, max_z],
            },
        );
    }

    /// Drop all cached GPU buffers. Call when a new file is opened.
    pub fn clear_mesh_cache(&mut self) {
        self.mesh_cache.clear();
    }

    fn white_texture_1x1(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
        let size = wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("white_1x1"),
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
            &[255u8, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        GpuTexture { texture, view, sampler }
    }
}
