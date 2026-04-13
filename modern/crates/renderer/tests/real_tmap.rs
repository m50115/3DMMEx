//! Integration test: TMAP pixelmap parsing + RGBA conversion against real content.
//!
//! Uses tmpls.3cn which contains TMAP chunks as children of MTRL chunks
//! (hierarchy: TMPL → CMTL → MTRL → TMAP).

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::tag::CTG_TMAP;
use engine::tmap::{BrTmap, BrPixelType};
use renderer::convert::tmap_to_rgba;

const TMPLS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/tmpls.3cn");

#[test]
fn parse_and_convert_tmap_chunks() {
    let file = File::open(TMPLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    let mut parsed   = 0u32;
    let mut converted = 0u32;
    let mut index8_count = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_TMAP) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let tmap = match BrTmap::from_bytes(&data) {
            Ok(t) => t,
            Err(_) => continue,
        };

        parsed += 1;

        // Dimensions must be positive
        assert!(tmap.width > 0,  "TMAP {:#010x}: width=0",  entry.id.cno);
        assert!(tmap.height > 0, "TMAP {:#010x}: height=0", entry.id.cno);
        // row_bytes must fit at least one row of pixels
        let bpp = tmap.pixel_type_parsed().bytes_per_pixel() as i16;
        assert!(
            tmap.row_bytes >= tmap.width * bpp,
            "TMAP {:#010x}: row_bytes={} < width({})×bpp({})",
            entry.id.cno, tmap.row_bytes, tmap.width, bpp,
        );
        // Pixel data length must match row_bytes×height
        assert_eq!(
            tmap.pixels.len(),
            tmap.pixel_data_len(),
            "TMAP {:#010x}: pixel buffer len mismatch",
            entry.id.cno,
        );

        if tmap.pixel_type_parsed() == BrPixelType::Index8 {
            index8_count += 1;
        }

        // Convert to RGBA
        let (w, h, rgba) = tmap_to_rgba(&tmap);
        assert_eq!(w, tmap.width as u32);
        assert_eq!(h, tmap.height as u32);
        assert_eq!(rgba.len(), w as usize * h as usize * 4,
                   "TMAP {:#010x}: wrong RGBA buffer size", entry.id.cno);

        // All alpha values must be 255 (opaque)
        for (i, chunk) in rgba.chunks(4).enumerate() {
            assert_eq!(chunk[3], 255, "TMAP {:#010x} pixel {i}: alpha != 255", entry.id.cno);
        }

        converted += 1;
    }

    assert!(
        parsed > 0,
        "No TMAP chunks found in {TMPLS_PATH} — check CTG_TMAP constant or content file path",
    );
    assert_eq!(parsed, converted, "Some TMAPs failed to convert");

    // 3DMM textures are almost exclusively INDEX_8
    assert!(
        index8_count > 0,
        "Expected at least some INDEX_8 TMAPs; got 0 out of {parsed}",
    );

    println!("TMAP integration: parsed={parsed}, converted={converted}, index8={index8_count}");
}
