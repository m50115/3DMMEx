//! Headless render-to-texture test.
//!
//! Renders one renderable model from tmpls.3cn to an offscreen RGBA8 texture,
//! reads back the pixels via a staging buffer, and asserts non-black output.
//! Skips gracefully when no GPU adapter is available (CI).

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::model::Model;
use engine::tag::CTG_BMDL;
use renderer::convert::model_to_mesh;
use renderer::material::{GpuMaterial, GpuTexture};
use renderer::pipeline::{GpuModelUniform, RenderPipeline};

const TMPLS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/tmpls.3cn"
);

// ---------------------------------------------------------------------------
// Headless GPU setup
// ---------------------------------------------------------------------------

fn try_headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
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
    Some((device, queue))
}

// ---------------------------------------------------------------------------
// 1×1 white fallback texture
// ---------------------------------------------------------------------------

fn white_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white"),
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
    GpuTexture {
        texture,
        view,
        sampler,
    }
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[test]
fn render_model_headless() {
    let Some((device, queue)) = try_headless_device() else {
        println!("render_model_headless: no GPU adapter — skipping");
        return;
    };

    // --- load first renderable model from tmpls.3cn ---
    let file = File::open(TMPLS_PATH).unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse tmpls.3cn");

    let model = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_BMDL)
        .find_map(|c| {
            let data = cfl.get_chunk_data(c.id.ctg, c.id.cno).ok()?;
            if data.len() < 80 {
                return None;
            }
            let m = Model::from_bytes(&data).ok()?;
            if m.has_valid_faces() {
                Some(m)
            } else {
                None
            }
        })
        .expect("no renderable model in tmpls.3cn");

    let mesh = model_to_mesh(&model);

    // Diagnostic: bounding box and camera coverage
    let (mut min_x, mut min_y, mut min_z) = (f32::MAX, f32::MAX, f32::MAX);
    let (mut max_x, mut max_y, mut max_z) = (f32::MIN, f32::MIN, f32::MIN);
    for v in &mesh.vertices {
        min_x = min_x.min(v.position[0]);
        max_x = max_x.max(v.position[0]);
        min_y = min_y.min(v.position[1]);
        max_y = max_y.max(v.position[1]);
        min_z = min_z.min(v.position[2]);
        max_z = max_z.max(v.position[2]);
    }
    println!(
        "mesh bbox: x=[{:.4},{:.4}] y=[{:.4},{:.4}] z=[{:.4},{:.4}], radius={}",
        min_x, max_x, min_y, max_y, min_z, max_z, mesh.radius
    );

    let width = 640u32;
    let height = 480u32;
    let render_format = wgpu::TextureFormat::Rgba8UnormSrgb;

    // --- color target ---
    let color_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());

    // --- depth target ---
    let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

    // --- pipeline ---
    let rp = RenderPipeline::new(&device, render_format, width, height);

    // --- camera: auto-fit to actual bounding box ---
    use renderer::camera::Camera;
    use renderer::lighting::GpuLight;
    let mut camera = Camera::new();
    camera.aspect = width as f32 / height as f32;
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let cz = (min_z + max_z) * 0.5;
    // Compute bounding sphere from bbox diagonal (header radius may be 0 for runtime models)
    let half_diag =
        ((max_x - min_x).powi(2) + (max_y - min_y).powi(2) + (max_z - min_z).powi(2)).sqrt() * 0.5;
    let fit_radius = half_diag.max(0.001);
    camera.target = glam::Vec3::new(cx, cy, cz);
    // Pull back enough to fit the whole sphere; look from +Z side
    camera.position = glam::Vec3::new(cx, cy, max_z + fit_radius * 2.0);
    camera.near = fit_radius * 0.01;
    camera.far = fit_radius * 20.0;
    println!(
        "camera: pos=({:.4},{:.4},{:.4}) target=({:.4},{:.4},{:.4}) fit_radius={:.4} near={:.6} far={:.4}",
        camera.position.x, camera.position.y, camera.position.z,
        camera.target.x, camera.target.y, camera.target.z,
        fit_radius, camera.near, camera.far
    );
    let gpu_cam = camera.to_gpu();
    let gpu_light = GpuLight::default();

    queue.write_buffer(&rp.camera_buf, 0, bytemuck::bytes_of(&gpu_cam));
    queue.write_buffer(&rp.light_buf, 0, bytemuck::bytes_of(&gpu_light));

    // --- mesh buffers ---
    use wgpu::util::DeviceExt;
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

    let fallback = white_texture(&device, &queue);
    let instance_bg = rp.create_instance_bind_group(
        &device,
        &model_buf,
        &material_buf,
        &fallback.view,
        &fallback.sampler,
    );

    // --- render pass ---
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
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
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
        rpass.set_bind_group(1, &instance_bg, &[]);
        rpass.set_vertex_buffer(0, vertex_buf.slice(..));
        rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..mesh.index_count(), 0, 0..1);
    }

    // --- readback via staging buffer ---
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
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let (tx, rx) = std::sync::mpsc::channel();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().expect("staging buffer map failed");

    let mapped = staging.slice(..).get_mapped_range();
    let pixels: &[u8] = &mapped;

    let non_black = pixels
        .chunks(4)
        .filter(|px| px[0] > 0 || px[1] > 0 || px[2] > 0)
        .count();

    println!(
        "render_model_headless: {}×{} frame, {} non-black pixels (mesh: {} verts, {} tris)",
        width,
        height,
        non_black,
        mesh.vertices.len(),
        mesh.indices.len() / 3,
    );

    assert!(
        non_black > 0,
        "rendered frame is entirely black — pipeline broken or mesh off-screen"
    );
}
