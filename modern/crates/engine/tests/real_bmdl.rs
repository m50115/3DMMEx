//! Integration test: load real BMDL chunks from content files.
//!
//! Reads tmpls.3cn (shipped with 3DMMEx), finds BMDL chunks,
//! decompresses them, and parses into engine::model::Model.
//!
//! Three categories of BMDL chunks exist in content files:
//!   1. Prepared models (radius > 0, bo=1) — face indices valid ✓
//!   2. Unprepared models (radius = 0, bo=1) — face data unreliable ✗
//!   3. Non-MODLF chunks (bo ≠ 1) — different format, skipped

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::model::Model;
use engine::tag::CTG_BMDL;

/// Path to the content file with template models.
const TMPLS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/tmpls.3cn");

fn open_tmpls() -> ChunkyFile {
    let file = File::open(TMPLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    ChunkyFile::read(&mut reader)
        .unwrap_or_else(|e| panic!("Cannot parse {TMPLS_PATH}: {e}"))
}

#[test]
fn tmpls_contains_bmdl_chunks() {
    let cfl = open_tmpls();
    let bmdl_count = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL).count();
    assert!(bmdl_count > 100, "Expected >100 BMDL chunks, got {bmdl_count}");
}

#[test]
fn models_with_valid_faces_convert_cleanly() {
    let cfl = open_tmpls();
    let mut valid = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 48 { continue; }

        let model = match Model::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if !model.has_valid_faces() { continue; }
        if model.header.vertex_count == 0 { continue; }

        assert_eq!(model.vertices.len(), model.header.vertex_count as usize);
        assert_eq!(model.faces.len(), model.header.face_count as usize);
        valid += 1;
    }

    assert!(
        valid > 150,
        "Expected >150 models with valid faces, found {valid}"
    );
}

#[test]
fn unprepared_models_parse_structurally() {
    let cfl = open_tmpls();
    let mut unprepared = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 48 { continue; }

        let model = match Model::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if model.is_prepared() { continue; }
        if model.header.vertex_count == 0 { continue; }

        // Structural checks: arrays have correct length
        assert_eq!(model.vertices.len(), model.header.vertex_count as usize);
        assert_eq!(model.faces.len(), model.header.face_count as usize);

        // Face indices are NOT expected to be valid for unprepared models
        unprepared += 1;
    }

    assert!(
        unprepared > 100,
        "Expected >100 unprepared models, found {unprepared}"
    );
}

#[test]
fn bmdl_radius_is_non_negative() {
    let cfl = open_tmpls();

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL).take(200) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 48 { continue; }

        if let Ok(model) = Model::from_bytes(&data) {
            assert!(
                model.header.radius.0 >= 0,
                "BMDL:{} has negative radius: {}",
                entry.id.cno,
                model.header.radius.to_f64()
            );
        }
    }
}

#[test]
fn chunk_category_breakdown() {
    let cfl = open_tmpls();
    let mut prepared = 0u32;
    let mut unprepared = 0u32;
    let mut non_modlf = 0u32;
    let mut empty = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => { non_modlf += 1; continue; }
        };
        if data.len() < 48 { non_modlf += 1; continue; }

        match Model::from_bytes(&data) {
            Ok(model) => {
                if model.header.vertex_count == 0 && model.header.face_count == 0 {
                    empty += 1;
                } else if model.is_prepared() {
                    prepared += 1;
                } else {
                    unprepared += 1;
                }
            }
            Err(_) => non_modlf += 1,
        }
    }

    let total = prepared + unprepared + non_modlf + empty;
    eprintln!("BMDL chunk breakdown ({total} total):");
    eprintln!("  Prepared (radius>0, faces valid):  {prepared}");
    eprintln!("  Unprepared (radius=0, faces bad):  {unprepared}");
    eprintln!("  Non-MODLF (bo≠1 or size mismatch): {non_modlf}");
    eprintln!("  Empty (cver=cfac=0):                {empty}");

    assert!(prepared > 150);
    assert!(total > 3000);
}
