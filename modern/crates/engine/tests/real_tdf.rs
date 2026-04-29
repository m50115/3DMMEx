//! Integration test: parse all TDF chunks from tdfs.3cn.
//!
//! Verifies:
//!   - All CTG_TDF chunks parse without error
//!   - Every parsed TDF has dxr.len() == cch and dyr.len() == cch
//!   - dyr_max > 0 for all fonts
//!   - At least one TDF has a BMDL child at chid == 65 ('A')
//!   - That BMDL parses as a Model with has_valid_faces() == true

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::model::Model;
use engine::tag::{CTG_BMDL, CTG_TDF};
use engine::tdf::BrTdf;

const TDFS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/tdfs.3cn"
);

fn open_tdfs() -> ChunkyFile {
    let file = File::open(TDFS_PATH).unwrap_or_else(|e| panic!("Cannot open {TDFS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    ChunkyFile::read(&mut reader).unwrap_or_else(|e| panic!("Cannot parse {TDFS_PATH}: {e}"))
}

#[test]
fn all_tdf_chunks_parse() {
    let cfl = open_tdfs();
    let tdf_chunks: Vec<_> = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_TDF).collect();
    assert!(
        !tdf_chunks.is_empty(),
        "No CTG_TDF chunks found in tdfs.3cn"
    );

    let mut parsed = 0u32;
    let mut errors = Vec::new();

    for chunk in &tdf_chunks {
        match cfl.get_chunk_data(CTG_TDF, chunk.id.cno) {
            Ok(data) => match BrTdf::from_bytes(&data) {
                Ok(tdf) => {
                    assert_eq!(
                        tdf.dxr.len(),
                        tdf.cch as usize,
                        "TDF:{} dxr.len()={} != cch={}",
                        chunk.id.cno,
                        tdf.dxr.len(),
                        tdf.cch
                    );
                    assert_eq!(
                        tdf.dyr.len(),
                        tdf.cch as usize,
                        "TDF:{} dyr.len()={} != cch={}",
                        chunk.id.cno,
                        tdf.dyr.len(),
                        tdf.cch
                    );
                    assert!(
                        tdf.dyr_max > 0.0,
                        "TDF:{} dyr_max={} not positive",
                        chunk.id.cno,
                        tdf.dyr_max
                    );
                    parsed += 1;
                }
                Err(e) => errors.push(format!("TDF:{} parse error: {e}", chunk.id.cno)),
            },
            Err(e) => errors.push(format!("TDF:{} data error: {e}", chunk.id.cno)),
        }
    }

    assert!(
        errors.is_empty(),
        "{} TDF parse errors:\n{}",
        errors.len(),
        errors.join("\n")
    );
    println!("Parsed {parsed} TDF chunks from tdfs.3cn");
}

#[test]
fn tdf_has_glyph_a_with_valid_faces() {
    let cfl = open_tdfs();
    let tdf_chunks: Vec<_> = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_TDF).collect();

    // Find any TDF that has a BMDL child at chid == 65 ('A')
    let glyph_a = tdf_chunks.iter().find_map(|tdf_chunk| {
        tdf_chunk
            .children
            .iter()
            .find(|ch| ch.id.ctg == CTG_BMDL && ch.chid == 65)
            .map(|ch| (tdf_chunk.id.cno, ch.id.cno))
    });

    let (tdf_cno, bmdl_cno) = glyph_a.expect(
        "No TDF chunk contains a BMDL child at chid==65 ('A') — tdfs.3cn may lack letter glyphs",
    );

    println!("Found glyph 'A' in TDF:{tdf_cno} → BMDL:{bmdl_cno}");

    let data = cfl
        .get_chunk_data(CTG_BMDL, bmdl_cno)
        .unwrap_or_else(|e| panic!("Cannot read BMDL:{bmdl_cno}: {e}"));

    let model =
        Model::from_bytes(&data).unwrap_or_else(|e| panic!("Cannot parse BMDL:{bmdl_cno}: {e}"));

    assert!(
        model.has_valid_faces(),
        "Glyph 'A' BMDL:{bmdl_cno} has no valid faces"
    );
}
