//! Integration test: upload a real TMAP pixelmap to the GPU.
//!
//! Creates a headless wgpu device (no surface/window) and uploads the first
//! TMAP chunk from tmpls.3cn via `tmap_to_texture()`.  Verifies that:
//!   - the texture dimensions match the parsed BrTmap
//!   - the wgpu::Texture, TextureView, and Sampler are created without panic
//!
//! If no GPU adapter is available (headless CI without a GPU), the test is
//! skipped with a message rather than failing.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::tag::CTG_TMAP;
use engine::tmap::BrTmap;
use renderer::convert::tmap_to_texture;

const TMPLS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/tmpls.3cn");

/// Try to create a headless wgpu device.  Returns `None` if no adapter is
/// available (no GPU on the machine or CI environment).
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
            label: Some("test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
        },
        None,
    ))
    .ok()?;

    Some((device, queue))
}

#[test]
fn upload_first_tmap_to_gpu() {
    let (device, queue) = match try_headless_device() {
        Some(dq) => dq,
        None => {
            eprintln!("real_tmap_upload: no GPU adapter — skipping");
            return;
        }
    };

    let file = File::open(TMPLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    // Find the first TMAP chunk that parses successfully.
    let tmap = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_TMAP)
        .find_map(|entry| {
            let data = cfl.get_chunk_data(entry.id.ctg, entry.id.cno).ok()?;
            BrTmap::from_bytes(&data).ok()
        })
        .expect("No parseable TMAP chunk found in tmpls.3cn");

    let expected_w = tmap.width.max(0) as u32;
    let expected_h = tmap.height.max(0) as u32;

    // Upload to GPU — must not panic.
    let gpu_tex = tmap_to_texture(&tmap, &device, &queue);

    // Verify texture dimensions.
    let size = gpu_tex.texture.size();
    assert_eq!(size.width,  expected_w, "texture width mismatch");
    assert_eq!(size.height, expected_h, "texture height mismatch");
    assert_eq!(size.depth_or_array_layers, 1, "expected a single layer");

    // Verify format.
    assert_eq!(
        gpu_tex.texture.format(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        "expected Rgba8UnormSrgb format",
    );

    println!(
        "real_tmap_upload: uploaded {}×{} TMAP to GPU ({})",
        expected_w, expected_h, gpu_tex.texture.format().describe_in_test(),
    );
}

/// Helper trait for a human-readable format name in test output.
trait FormatName {
    fn describe_in_test(&self) -> &'static str;
}
impl FormatName for wgpu::TextureFormat {
    fn describe_in_test(&self) -> &'static str {
        match self {
            wgpu::TextureFormat::Rgba8UnormSrgb => "Rgba8UnormSrgb",
            wgpu::TextureFormat::Rgba8Unorm     => "Rgba8Unorm",
            _                                   => "other",
        }
    }
}
