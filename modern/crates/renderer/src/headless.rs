//! Headless (offscreen) GPU renderer.
//!
//! Provides a `HeadlessRenderer` that renders an `engine::Model` to raw RGBA8
//! pixels without a window. Used by the Tauri backend to produce viewport
//! frames as PNG images.
//!
//! Returns `None` / gracefully skips when no GPU adapter is available (CI, no GPU).

use engine::model::Model;
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
}

impl HeadlessRenderer {
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
        Some(Self { device, queue })
    }

    /// Render `model` to an offscreen `width × height` texture.
    ///
    /// Returns raw RGBA8 pixel data (row-major, `width * height * 4` bytes).
    /// Returns an empty `Vec` if the model has no renderable faces.
    pub fn render_model(&self, model: &Model, width: u32, height: u32) -> Vec<u8> {
        if !model.has_valid_faces() || model.vertices.is_empty() {
            return Vec::new();
        }

        let device = &self.device;
        let queue = &self.queue;
        let render_format = wgpu::TextureFormat::Rgba8UnormSrgb;

        // ── pipeline ──────────────────────────────────────────────────────────
        let rp = RenderPipeline::new(device, render_format, width, height);

        // ── mesh ──────────────────────────────────────────────────────────────
        let mesh = model_to_mesh(model);

        // ── auto-fit camera ───────────────────────────────────────────────────
        let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
        for v in &mesh.vertices {
            min_x = min_x.min(v.position[0]); max_x = max_x.max(v.position[0]);
            min_y = min_y.min(v.position[1]); max_y = max_y.max(v.position[1]);
            min_z = min_z.min(v.position[2]); max_z = max_z.max(v.position[2]);
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

        // ── GPU buffers ───────────────────────────────────────────────────────
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vbuf"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ibuf"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let model_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model_uniform"),
            contents: bytemuck::bytes_of(&GpuModelUniform::identity()),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let material_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material_uniform"),
            contents: bytemuck::bytes_of(&GpuMaterial::default()),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let fallback = Self::white_texture_1x1(device, queue);
        let instance_bg = rp.create_instance_bind_group(
            device,
            &model_buf,
            &material_buf,
            &fallback.view,
            &fallback.sampler,
        );

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
            rpass.set_bind_group(1, &instance_bg, &[]);
            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
            rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
            rpass.draw_indexed(0..mesh.index_count(), 0, 0..1);
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
