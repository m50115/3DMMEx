//! Integration test: parse real MIDS chunks from snds.3cn.
//!
//! Loads MIDS (MIDI) chunks and verifies they are valid Standard MIDI Files.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::tag::CTG_MIDS;

use audio::parse_midi;

const SNDS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/snds.3cn"
);

#[test]
fn first_mids_chunk_parses() {
    let file = File::open(SNDS_PATH).unwrap_or_else(|e| panic!("Cannot open {SNDS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse snds.3cn");

    let mids_chunk = cfl
        .chunks
        .iter()
        .find(|c| c.id.ctg == CTG_MIDS)
        .expect("no MIDS chunks in snds.3cn");

    let data = cfl
        .get_chunk_data(mids_chunk.id.ctg, mids_chunk.id.cno)
        .expect("get MIDS data");

    println!(
        "MIDS cno={} size={} magic={:?}",
        mids_chunk.id.cno,
        data.len(),
        &data[..data.len().min(4)]
    );

    let info = parse_midi(&data).expect("parse MIDI");

    println!(
        "MIDI: format={} tracks={} events={}",
        info.format, info.track_count, info.event_count
    );

    assert!(info.track_count > 0, "no MIDI tracks");
    assert!(info.event_count > 0, "no MIDI events");
}

#[test]
fn batch_mids_chunks_parse() {
    let file = File::open(SNDS_PATH).unwrap_or_else(|e| panic!("Cannot open {SNDS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse snds.3cn");

    let mids_chunks: Vec<_> = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_MIDS)
        .take(50)
        .collect();

    assert!(!mids_chunks.is_empty(), "no MIDS chunks found");

    let mut ok = 0usize;
    let mut errors = 0usize;

    for chunk in &mids_chunks {
        let data = match cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno) {
            Ok(d) => d,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        match parse_midi(&data) {
            Ok(info) => {
                assert!(info.track_count > 0);
                ok += 1;
            }
            Err(e) => {
                eprintln!("MIDS cno={} parse error: {e}", chunk.id.cno);
                errors += 1;
            }
        }
    }

    println!(
        "batch_mids_chunks_parse: {ok} OK, {errors} errors out of {}",
        mids_chunks.len()
    );

    assert_eq!(errors, 0, "{errors} MIDS chunks failed to parse");
}
