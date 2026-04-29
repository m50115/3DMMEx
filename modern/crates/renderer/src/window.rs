//! Windowed renderer using winit + wgpu.
//!
//! Opens a window and renders a `RenderWorld` each frame.

use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::camera::Camera;
use crate::lighting::GpuLight;
use crate::material::{GpuMaterial, GpuTexture};
use crate::pipeline::{GpuModelUniform, RenderPipeline};
use crate::vertex::{GpuVertex, Mesh};

/// GPU-uploaded mesh ready for drawing.
struct GpuMesh {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    instance_bind_group: wgpu::BindGroup,
}

/// All GPU state, created after the window surface is available.
struct GpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    render_pipeline: RenderPipeline,
    gpu_meshes: Vec<GpuMesh>,
    camera: Camera,
    light: GpuLight,
    clear_color: wgpu::Color,
    /// 1×1 white fallback for untextured materials.
    fallback_texture: GpuTexture,
}

impl GpuState {
    fn new(window: &'static Window) -> Self {
        let size = window.inner_size();
        let w = size.width.max(1);
        let h = size.height.max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window).expect("create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("3dmm_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))
        .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let render_pipeline = RenderPipeline::new(&device, surface_format, w, h);

        let mut camera = Camera::new();
        camera.aspect = w as f32 / h as f32;

        let fallback_texture = create_white_texture(&device, &queue);

        Self {
            device,
            queue,
            surface,
            surface_config,
            render_pipeline,
            gpu_meshes: Vec::new(),
            camera,
            light: GpuLight::default(),
            clear_color: wgpu::Color {
                r: 0.1,
                g: 0.1,
                b: 0.15,
                a: 1.0,
            },
            fallback_texture,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        let w = new_size.width.max(1);
        let h = new_size.height.max(1);
        self.surface_config.width = w;
        self.surface_config.height = h;
        self.surface.configure(&self.device, &self.surface_config);
        self.render_pipeline.resize(&self.device, w, h);
        self.camera.aspect = w as f32 / h as f32;
    }

    /// Upload a mesh with identity transform and default material.
    fn upload_mesh(&mut self, mesh: &Mesh) {
        let vertex_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex_buf"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let index_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("index_buf"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let model_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("model_uniform"),
                contents: bytemuck::bytes_of(&GpuModelUniform::identity()),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let material_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("material_uniform"),
                contents: bytemuck::bytes_of(&GpuMaterial::default()),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let instance_bind_group = self.render_pipeline.create_instance_bind_group(
            &self.device,
            &model_buf,
            &material_buf,
            &self.fallback_texture.view,
            &self.fallback_texture.sampler,
        );

        self.gpu_meshes.push(GpuMesh {
            vertex_buf,
            index_buf,
            index_count: mesh.index_count(),
            instance_bind_group,
        });
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update per-frame uniforms
        let gpu_camera = self.camera.to_gpu();
        self.queue.write_buffer(
            &self.render_pipeline.camera_buf,
            0,
            bytemuck::bytes_of(&gpu_camera),
        );
        self.queue.write_buffer(
            &self.render_pipeline.light_buf,
            0,
            bytemuck::bytes_of(&self.light),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.render_pipeline.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_pipeline(&self.render_pipeline.pipeline);
            rpass.set_bind_group(0, &self.render_pipeline.frame_bind_group, &[]);

            for gm in &self.gpu_meshes {
                rpass.set_bind_group(1, &gm.instance_bind_group, &[]);
                rpass.set_vertex_buffer(0, gm.vertex_buf.slice(..));
                rpass.set_index_buffer(gm.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..gm.index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// winit application handler
// ---------------------------------------------------------------------------

struct App {
    state: Option<GpuState>,
    /// Mesh to upload once the window is ready.
    pending_mesh: Option<Mesh>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("3DMMEx Renderer")
            .with_inner_size(PhysicalSize::new(1280u32, 720));

        let window = event_loop.create_window(attrs).expect("create window");
        // Leak the window so we get a 'static reference for the surface.
        let window: &'static Window = Box::leak(Box::new(window));

        let mut gpu = GpuState::new(window);

        if let Some(mesh) = self.pending_mesh.take() {
            gpu.upload_mesh(&mesh);
        }

        self.state = Some(gpu);
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => {
                state.resize(new_size);
            }
            WindowEvent::RedrawRequested => match state.render() {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost) => {
                    let size =
                        PhysicalSize::new(state.surface_config.width, state.surface_config.height);
                    state.resize(size);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                Err(e) => log::warn!("render error: {e:?}"),
            },
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a 1×1 opaque white texture used as a fallback for untextured materials.
fn create_white_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fallback_white"),
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
        label: Some("fallback_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    GpuTexture {
        texture,
        view,
        sampler,
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open a window and render the given mesh until the user closes it.
pub fn run_with_mesh(mesh: Mesh) {
    env_logger::init();
    let event_loop = EventLoop::new().expect("create event loop");
    let mut app = App {
        state: None,
        pending_mesh: Some(mesh),
    };
    event_loop.run_app(&mut app).expect("event loop");
}

/// Open a window showing a test triangle.
pub fn run_test_triangle() {
    run_with_mesh(test_triangle());
}

/// A simple colored triangle for smoke-testing the pipeline.
pub fn test_triangle() -> Mesh {
    let white = [1.0, 1.0, 1.0, 1.0];
    let up = [0.0, 0.0, 1.0]; // facing camera

    let vertices = vec![
        GpuVertex {
            position: [0.0, 0.5, 0.0],
            normal: up,
            uv: [0.5, 0.0],
            color: [1.0, 0.2, 0.2, 1.0], // red
        },
        GpuVertex {
            position: [-0.5, -0.5, 0.0],
            normal: up,
            uv: [0.0, 1.0],
            color: [0.2, 1.0, 0.2, 1.0], // green
        },
        GpuVertex {
            position: [0.5, -0.5, 0.0],
            normal: up,
            uv: [1.0, 1.0],
            color: white,
        },
    ];

    let indices = vec![0, 1, 2];

    Mesh::new(vertices, indices, 1.0)
}
