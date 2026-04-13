//! Integration tests: BKGD/CAM/GLLT parsing from real content files.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::background::{BrBackground, BrCamera, BrLightList, BKGDF_SIZE, CAM_SIZE};
use engine::error::EngineError;
use engine::tag::{CTG_BKGD, CTG_CAM, CTG_GLLT};

const BKGDS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/bkgds.3cn");

fn open_bkgds() -> ChunkyFile {
    let f = File::open(BKGDS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {BKGDS_PATH}: {e}"));
    ChunkyFile::read(&mut BufReader::new(f)).unwrap()
}

// ── BKGD header ────────────────────────────────────────────────────────────

#[test]
fn bkgds_contains_bkgd_chunks() {
    let cfl = open_bkgds();
    let count = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BKGD).count();
    assert!(count > 0, "No BKGD chunks found in bkgds.3cn");
}

#[test]
fn all_bkgd_headers_parse() {
    let cfl = open_bkgds();
    let mut ok = 0u32;
    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BKGD) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < BKGDF_SIZE {
            continue;
        }
        BrBackground::from_bytes(&data).unwrap();
        ok += 1;
    }
    assert!(ok > 0, "No BKGD headers parsed");
}

// ── CAM chunks ─────────────────────────────────────────────────────────────

#[test]
fn bkgds_contains_cam_chunks() {
    let cfl = open_bkgds();
    let count = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_CAM).count();
    assert!(count > 10, "Expected many CAM chunks, got {count}");
}

#[test]
fn all_cam_chunks_parse() {
    let cfl = open_bkgds();
    let mut ok = 0u32;
    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_CAM) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < CAM_SIZE {
            continue;
        }
        // A small number of CAM chunks use bo=4 (panoramic/special cameras).
        // Skip those — they use a different sub-format not needed for basic rendering.
        let cam = match BrCamera::from_bytes(&data) {
            Ok(c) => c,
            Err(EngineError::InvalidByteOrder(_)) => continue,
            Err(e) => panic!("CAM cno={} unexpected error: {e}", entry.id.cno),
        };

        // Sanity: FOV must be > 0
        assert!(cam.a_fov > 0, "CAM cno={}: a_fov is zero", entry.id.cno);
        // Sanity: yon_z > hither_z (far > near)
        assert!(
            cam.yon_z.0 > cam.hither_z.0,
            "CAM cno={}: yon_z ({}) ≤ hither_z ({})",
            entry.id.cno,
            cam.yon_z.0,
            cam.hither_z.0
        );
        ok += 1;
    }
    assert!(ok > 10, "Only parsed {ok} CAM chunks");
}

#[test]
fn cam_fov_in_valid_range() {
    let cfl = open_bkgds();
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
            Err(EngineError::InvalidByteOrder(_)) => continue,
            Err(e) => panic!("CAM cno={} unexpected error: {e}", entry.id.cno),
        };
        let fov = cam.fov_radians();
        assert!(
            fov > 0.0 && fov < std::f32::consts::PI,
            "CAM cno={}: fov={fov:.4} rad out of (0, π)",
            entry.id.cno
        );
    }
}

#[test]
fn cam_roundtrip() {
    let cfl = open_bkgds();
    let mut tested = 0u32;
    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_CAM).take(5) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < CAM_SIZE {
            continue;
        }
        let cam = match BrCamera::from_bytes(&data) {
            Ok(c) => c,
            Err(EngineError::InvalidByteOrder(_)) => continue,
            Err(e) => panic!("CAM cno={} unexpected error: {e}", entry.id.cno),
        };
        let reser = cam.to_bytes();
        let cam2 = BrCamera::from_bytes(&reser).unwrap();
        assert_eq!(cam, cam2, "Round-trip failed for CAM cno={}", entry.id.cno);
        tested += 1;
    }
    assert!(tested > 0, "No CAM chunks round-tripped");
}

// ── GLLT chunks ────────────────────────────────────────────────────────────

#[test]
fn bkgds_contains_gllt_chunks() {
    let cfl = open_bkgds();
    let count = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_GLLT).count();
    assert!(count > 0, "No GLLT chunks found in bkgds.3cn");
}

#[test]
fn all_gllt_chunks_parse() {
    let cfl = open_bkgds();
    let mut ok = 0u32;
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

        // Each background should have ≥ 1 light
        assert!(
            !list.lights.is_empty(),
            "GLLT cno={}: empty light list",
            entry.id.cno
        );

        // Intensity must be non-negative
        for (i, lit) in list.lights.iter().enumerate() {
            assert!(
                lit.intensity_f32() >= 0.0,
                "GLLT cno={} light[{i}]: negative intensity",
                entry.id.cno
            );
        }
        ok += 1;
    }
    assert!(ok > 0, "No GLLT chunks parsed");
}
