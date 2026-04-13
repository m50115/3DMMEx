//! Integration test: CAM + GLLT → renderer Camera + GpuLight pipeline.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::background::{BrCamera, BrLightList, CAM_SIZE};
use engine::error::EngineError;
use engine::tag::{CTG_CAM, CTG_GLLT};
use renderer::convert::{camera_to_renderer, light_to_renderer};

const BKGDS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/bkgds.3cn");

fn open_bkgds() -> ChunkyFile {
    let f = File::open(BKGDS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {BKGDS_PATH}: {e}"));
    ChunkyFile::read(&mut BufReader::new(f)).unwrap()
}

#[test]
fn convert_all_cams_to_renderer() {
    let cfl = open_bkgds();
    let mut converted = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_CAM) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < CAM_SIZE {
            continue;
        }
        let cam = match BrCamera::from_bytes(&data) {
            Ok(c) => c,
            Err(EngineError::InvalidByteOrder(_)) => continue, // skip special-format cameras
            Err(e) => panic!("CAM cno={} unexpected error: {e}", entry.id.cno),
        };
        let r_cam = camera_to_renderer(&cam, 16.0 / 9.0);

        // FOV must be positive and < π
        assert!(
            r_cam.fov_y > 0.0 && r_cam.fov_y < std::f32::consts::PI,
            "fov_y={} out of range",
            r_cam.fov_y
        );
        // Near < Far
        assert!(r_cam.near < r_cam.far, "near={} ≥ far={}", r_cam.near, r_cam.far);
        // Near must be positive
        assert!(r_cam.near > 0.0, "near={} ≤ 0", r_cam.near);
        // Aspect set correctly
        assert!((r_cam.aspect - 16.0 / 9.0).abs() < 1e-5);

        converted += 1;
    }

    assert!(converted > 10, "Only converted {converted} CAM chunks");
}

#[test]
fn convert_all_gllts_to_renderer() {
    let cfl = open_bkgds();
    let mut converted = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_GLLT) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let list = match BrLightList::from_bytes(&data) {
            Ok(l) => l,
            Err(EngineError::InvalidByteOrder(_)) => continue,
            Err(e) => panic!("GLLT cno={} unexpected error: {e}", entry.id.cno),
        };

        for lit in &list.lights {
            let gpu_light = light_to_renderer(lit);

            // Intensity non-negative
            assert!(gpu_light.intensity >= 0.0, "intensity={}", gpu_light.intensity);
            // Color channels in [0, 1]
            for &c in &gpu_light.color {
                assert!((0.0..=1.0).contains(&c), "color channel={c}");
            }
            // Direction vector components are finite
            for &d in &gpu_light.direction {
                assert!(d.is_finite(), "direction component not finite: {d}");
            }
        }
        converted += 1;
    }

    assert!(converted > 0, "No GLLT chunks converted");
}
