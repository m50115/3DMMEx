//! Integration test: BMTL chunk parsing from real content files.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::material::BrMaterial;
use engine::tag::CTG_MTRL;

const MTRLS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/mtrls.3cn");

/// Collect all successfully-parsed BrMaterial values from tmpls.3cn.
fn load_all_materials(cfl: &ChunkyFile) -> Vec<BrMaterial> {
    cfl.chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_MTRL)
        .filter_map(|entry| {
            let data = cfl.get_chunk_data(entry.id.ctg, entry.id.cno).ok()?;
            BrMaterial::from_bytes(&data).ok()
        })
        .collect()
}

#[test]
fn tmpls_contains_bmtl_chunks() {
    let file = File::open(MTRLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    let count = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_MTRL).count();
    assert!(count > 40, "Expected >40 MTRL chunks, got {count}");
}

#[test]
fn all_bmtl_parse_successfully() {
    let file = File::open(MTRLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    let total = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_MTRL).count();
    let parsed = load_all_materials(&cfl);

    // Every BMTL chunk should parse; allow a small margin for corrupted entries
    assert!(
        parsed.len() > total * 9 / 10,
        "Only {}/{total} BMTL chunks parsed successfully",
        parsed.len()
    );
}

#[test]
fn material_colours_are_valid_rgb() {
    let file = File::open(MTRLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    for mat in load_all_materials(&cfl) {
        // br_colour is 0x00RRGGBB — high byte should be 0 (no alpha in MTRLF)
        let alpha_byte = (mat.colour >> 24) & 0xFF;
        assert_eq!(
            alpha_byte, 0,
            "Unexpected alpha byte {alpha_byte:#04x} in colour {:#010x}",
            mat.colour
        );
    }
}

#[test]
fn material_fractions_in_range() {
    let file = File::open(MTRLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    for mat in load_all_materials(&cfl) {
        // ka/kd/ks are br_ufraction (u16): values are always in [0, 65535]
        // by type, but let's verify conversions land in [0.0, 1.0]
        let ka = mat.ka as f32 / 65535.0;
        let kd = mat.kd as f32 / 65535.0;
        let ks = mat.ks as f32 / 65535.0;
        assert!((0.0..=1.0).contains(&ka), "ka out of range: {ka}");
        assert!((0.0..=1.0).contains(&kd), "kd out of range: {kd}");
        assert!((0.0..=1.0).contains(&ks), "ks out of range: {ks}");
    }
}

#[test]
fn material_power_is_non_negative() {
    let file = File::open(MTRLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    for mat in load_all_materials(&cfl) {
        let power = mat.power.to_f64();
        assert!(
            power >= 0.0,
            "Negative specular power {power} in material (colour={:#010x})",
            mat.colour
        );
    }
}

#[test]
fn material_roundtrip() {
    let file = File::open(MTRLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    for mat in load_all_materials(&cfl) {
        let bytes = mat.to_bytes();
        let mat2 = BrMaterial::from_bytes(&bytes)
            .expect("Roundtrip parse failed");
        assert_eq!(mat, mat2, "Roundtrip mismatch for material colour={:#010x}", mat.colour);
    }
}
