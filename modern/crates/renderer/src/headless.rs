//! Headless (offscreen) GPU renderer.
//!
//! Provides a `HeadlessRenderer` that renders an `engine::Model` to raw RGBA8
//! pixels without a window. Used by the Tauri backend to produce viewport
//! frames as PNG images.
//!
//! Returns `None` / gracefully skips when no GPU adapter is available (CI, no GPU).

use engine::model::Model;
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::convert::model_to_mesh;
use crate::lighting::GpuLight;
use crate::material::{GpuMaterial, GpuTexture};
use crate::pipeline::{GpuModelUniform, RenderPipeline};

/// Headless GPU context for offscreen rendering.
pub struct HeadlessRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Render pipeline — created once, reused across all render calls.
    pipeline: RenderPipeline,
    /// 1×1 white fallback texture — created once, shared across draw calls.
    fallback_texture: GpuTexture,
}

impl HeadlessRenderer {
    const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    /// Try to acquire a GPU adapter and create the headless renderer.
    /// Returns `None` if no adapter is available (no GPU, CI environment, etc.).
    pub fn try_new() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("headless_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: Default::default(),
            },
            None,
        ))
        .ok()?;
        // Pipeline is size-independent (depth texture created per call); 640×480 is the
        // default viewport size — RenderPipeline::resize() can update if needed.
        let pipeline = RenderPipeline::new(&device, Self::DEFAULT_FORMAT, 640, 480);
        let fallback_texture = Self::white_texture_1x1(&device, &queue);
        Some(Self { device, queue, pipeline, fallback_texture })
    }

    /// Render a single `model` to an offscreen `width × height` texture.
    ///
    /// Thin wrapper around [`render_models`] with an identity transform.
    /// Returns an empty `Vec` if the model has no renderable faces.
    pub fn render_model(&self, model: &Model, width: u32, height: u32) -> Vec<u8> {
        self.render_models(&[(model, Mat4::IDENTITY)], width, height)
    }

    /// Render one or more models into a single offscreen `width × height` frame.
    ///
    /// Each entry is `(model, world_transform)`. The camera auto-fits the
    /// combined world-space bounding box of all models.
    /// Returns raw RGBA8 pixel data (`width * height * 4` bytes), or an empty
    /// `Vec` if no model has renderable geometry.
    pub fn render_models(&self, models: &[(&Model, Mat4)], width: u32, height: u32) -> Vec<u8> {
        // Filter to renderable models only.
        let renderable: Vec<(&Model, Mat4)> = models.iter()
            .filter(|(m, _)| m.has_valid_faces() && !m.vertices.is_empty())
            .map(|(m, t)| (*m, *t))
            .collect();
        if renderable.is_empty() {
            return Vec::new();
        }

        let device = &self.device;
        let queue = &self.queue;
        let render_format = Self::DEFAULT_FORMAT;

        // ── pipeline (cached, reused every call) ──────────────────────────────
        let rp = &self.pipeline;

        // ── meshes + combined world-space bounding box ────────────────────────
        let mesh_transforms: Vec<_> = renderable.iter()
            .map(|(m, t)| (model_to_mesh(m), *t))
            .collect();

        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
        for (mesh, transform) in &mesh_transforms {
            for v in &mesh.vertices {
                let wp = transform.transform_point3(glam::Vec3::from_array(v.position));
                min_x = min_x.min(wp.x); max_x = max_x.max(wp.x);
                min_y = min_y.min(wp.y); max_y = max_y.max(wp.y);
                min_z = min_z.min(wp.z); max_z = max_z.max(wp.z);
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
        struct DrawCall {
            vertex_buf: wgpu::Buffer,
            index_buf: wgpu::Buffer,
            instance_bg: wgpu::BindGroup,
            index_count: u32,
        }

        let draw_calls: Vec<DrawCall> = mesh_transforms.iter().enumerate().map(|(i, (mesh, transform))| {
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
            DrawCall { vertex_buf, index_buf, instance_bg, index_count }
        }).collect();

        // ── offscreen targets ─────────────────────────────────────────────────
        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
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
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.08, g: 0.08, b: 0.12, a: 1.0 }),
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
            for dc in &draw_calls {
                rpass.set_bind_group(1, &dc.instance_bg, &[]);
                rpass.set_vertex_buffer(0, dc.vertex_buf.slice(..));
                rpass.set_index_buffer(dc.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..dc.index_count, 0, 0..1);
            }
        }

        // ── readback ──────────────────────────────────────────────────────────
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

        let (tx, rx) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);
        if rx.recv().unwrap().is_err() {
            return Vec::new();
        }

        let mapped = staging.slice(..).get_mapped_range();
        // The staging buffer rows may have padding; extract the actual pixel rows.
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let row_start = (row * bytes_per_row) as usize;
            let row_end = row_start + (width * 4) as usize;
            rgba.extend_from_slice(&mapped[row_start..row_end]);
        }
        rgba
    }

    fn white_texture_1x1(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
        let size = wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("white_1x1"),
            size,
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture, mip_level: 0,
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
