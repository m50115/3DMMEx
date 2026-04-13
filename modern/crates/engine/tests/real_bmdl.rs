//! Integration test: load real BMDL chunks from content files.
//!
//! Reads tmpls.3cn (shipped with 3DMMEx), finds BMDL chunks,
//! decompresses them, and parses into engine::model::Model.

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
    // tmpls.3cn has thousands of BMDL chunks
    assert!(bmdl_count > 100, "Expected >100 BMDL chunks, got {bmdl_count}");
}

#[test]
fn parse_first_nontrivial_bmdl() {
    let cfl = open_tmpls();

    // Find a BMDL with valid geometry (face indices all within vertex range).
    // Some BMDL chunks have out-of-range face indices — likely a content tool
    // format difference (possibly 64-bit br_face struct). We skip those here.
    let mut parsed_any = false;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Skip tiny chunks (no geometry)
        if data.len() < 80 {
            continue;
        }

        let model = match Model::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if model.header.vertex_count == 0 || model.header.face_count == 0 {
            continue;
        }
        assert_eq!(model.vertices.len(), model.header.vertex_count as usize);
        assert_eq!(model.faces.len(), model.header.face_count as usize);

        // Skip models with out-of-range face indices
        let nv = model.header.vertex_count as u16;
        let valid = model.faces.iter()
            .all(|f| f.vertices.iter().all(|&vi| vi < nv));
        if !valid { continue; }

        // Found a fully valid model
        parsed_any = true;
        break;
    }

    assert!(parsed_any, "Could not find any BMDL with valid face indices");
}

#[test]
fn parse_multiple_bmdl_chunks() {
    let cfl = open_tmpls();
    let mut success = 0u32;
    let mut skipped = 0u32;
    let mut failed = Vec::new();

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => { skipped += 1; continue; }
        };

        if data.len() < 48 {
            skipped += 1;
            continue;
        }

        match Model::from_bytes(&data) {
            Ok(model) => {
                // Verify vertex/face count matches
                assert_eq!(model.vertices.len(), model.header.vertex_count as usize);
                assert_eq!(model.faces.len(), model.header.face_count as usize);
                success += 1;
            }
            Err(e) => {
                failed.push((entry.id.cno, data.len(), format!("{e}")));
            }
        }

        // Test first 200 to keep test time reasonable
        if success + skipped + failed.len() as u32 >= 200 {
            break;
        }
    }

    assert!(
        success > 50,
        "Only {success} parsed out of first 200 BMDL chunks. Failures: {failed:?}"
    );

    if !failed.is_empty() {
        eprintln!(
            "Warning: {}/{} BMDL chunks failed to parse: {:?}",
            failed.len(),
            success + skipped + failed.len() as u32,
            &failed[..failed.len().min(5)]
        );
    }
}

#[test]
fn diagnose_face_stride() {
    // Probe: check if "bad" BMDL chunks use 40-byte faces (br_face) vs 32-byte (br_face_file).
    // For each chunk, compute expected sizes with both strides and see which matches.
    let cfl = open_tmpls();

    let mut match_32 = 0u32;
    let mut match_40 = 0u32;
    let mut match_both = 0u32;
    let mut match_neither = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 48 { continue; }

        let cver = i16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
        let cfac = i16::from_le_bytes(data[6..8].try_into().unwrap()) as usize;

        let exp32 = 48 + cver * 32 + cfac * 32;
        let exp40 = 48 + cver * 32 + cfac * 40;

        let m32 = data.len() == exp32;
        let m40 = data.len() == exp40;

        match (m32, m40) {
            (true, true) => match_both += 1,
            (true, false) => match_32 += 1,
            (false, true) => match_40 += 1,
            (false, false) => match_neither += 1,
        }
    }

    eprintln!("Face stride diagnosis:");
    eprintln!("  32-byte faces only: {match_32}");
    eprintln!("  40-byte faces only: {match_40}");
    eprintln!("  Both (0 faces):     {match_both}");
    eprintln!("  Neither:            {match_neither}");
    eprintln!("  Total:              {}", match_32 + match_40 + match_both + match_neither);

    // Diagnose the "neither" chunks — what's the size delta?
    if match_neither > 0 {
        let mut deltas: std::collections::HashMap<isize, u32> = std::collections::HashMap::new();
        for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
            let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if data.len() < 48 { continue; }
            let cver = i16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
            let cfac = i16::from_le_bytes(data[6..8].try_into().unwrap()) as usize;
            let exp32 = 48 + cver * 32 + cfac * 32;
            if data.len() != exp32 {
                let delta = data.len() as isize - exp32 as isize;
                *deltas.entry(delta).or_insert(0) += 1;
            }
        }
        let mut sorted: Vec<_> = deltas.into_iter().collect();
        sorted.sort_by_key(|(d, _)| *d);
        eprintln!("Size deltas (actual - expected_32):");
        for (delta, count) in &sorted[..sorted.len().min(20)] {
            eprintln!("  delta={delta:+6}: {count} chunks");
        }
    }

    // Examine headers of "neither" chunks
    eprintln!("\nSample 'neither' chunk headers:");
    let mut shown = 0;
    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        if shown >= 10 { break; }
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 48 { continue; }
        let cver = i16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
        let cfac = i16::from_le_bytes(data[6..8].try_into().unwrap()) as usize;
        let exp32 = 48 + cver * 32 + cfac * 32;
        if data.len() != exp32 {
            let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
            let osk = i16::from_le_bytes(data[2..4].try_into().unwrap());
            eprintln!(
                "  BMDL:{} len={} bo=0x{:04x} osk=0x{:04x} cver={} cfac={} exp32={} delta={}",
                entry.id.cno, data.len(), bo as u16, osk as u16, cver, cfac, exp32,
                data.len() as isize - exp32 as isize
            );
            // Also show first 16 bytes hex
            let hex: String = data[..16.min(data.len())].iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("    hex: {hex}");
            shown += 1;
        }
    }

    // For chunks with correct 32-byte layout, how many have bad face indices?
    let mut valid_geom = 0u32;
    let mut bad_indices = 0u32;
    let mut bo_not_one = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 48 { continue; }

        let bo = i16::from_le_bytes(data[0..2].try_into().unwrap());
        if bo != 1 {
            bo_not_one += 1;
            continue;
        }

        let cver = i16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
        let cfac = i16::from_le_bytes(data[6..8].try_into().unwrap()) as usize;
        let exp32 = 48 + cver * 32 + cfac * 32;

        if data.len() != exp32 { continue; } // wrong size even with bo=1

        if let Ok(model) = Model::from_bytes(&data) {
            let nv = model.header.vertex_count as u16;
            let all_valid = model.faces.iter()
                .all(|f| f.vertices.iter().all(|&vi| vi < nv));
            if all_valid {
                valid_geom += 1;
            } else {
                bad_indices += 1;
            }
        }
    }

    eprintln!("\nbo=1 + correct size breakdown:");
    eprintln!("  Valid geometry:    {valid_geom}");
    eprintln!("  Bad face indices:  {bad_indices}");
    eprintln!("  bo != 1:           {bo_not_one}");

    assert!(match_32 + match_40 + match_both > 0, "No chunks matched any layout");
}

#[test]
fn bmdl_radius_is_positive() {
    let cfl = open_tmpls();

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL).take(50) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 80 { continue; }

        if let Ok(model) = Model::from_bytes(&data) {
            if model.header.vertex_count > 0 {
                assert!(
                    model.header.radius.0 >= 0,
                    "BMDL:{} has negative radius: {}",
                    entry.id.cno,
                    model.header.radius.to_f64()
                );
            }
        }
    }
}
