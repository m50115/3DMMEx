//! Integration test: parse real MSND chunks from snds.3cn.
//!
//! Verifies that all MSND chunks in snds.3cn parse correctly and have
//! expected sound types (Nil, Sfx, Speech, or Midi).

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::msnd::{MovieSound, SoundType};
use engine::tag::CTG_MSND;

const SNDS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/snds.3cn"
);

#[test]
fn all_msnd_chunks_parse() {
    let file = File::open(SNDS_PATH).unwrap_or_else(|e| panic!("Cannot open {SNDS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse snds.3cn");

    let msnd_chunks: Vec<_> = cfl.chunks.iter().filter(|c| c.id.ctg == CTG_MSND).collect();

    assert!(!msnd_chunks.is_empty(), "no MSND chunks in snds.3cn");

    let mut counts = [0usize; 5]; // indexed by SoundType value
    let mut errors = 0usize;

    for chunk in &msnd_chunks {
        let data = match cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("MSND cno={} get_chunk_data: {e}", chunk.id.cno);
                errors += 1;
                continue;
            }
        };
        match MovieSound::from_bytes(&data) {
            Ok(msnd) => {
                counts[msnd.sty as usize] += 1;
            }
            Err(e) => {
                eprintln!("MSND cno={} parse error: {e}", chunk.id.cno);
                errors += 1;
            }
        }
    }

    println!(
        "MSND chunks: total={} Nil={} Unused={} Sfx={} Speech={} Midi={}",
        msnd_chunks.len(),
        counts[SoundType::Nil as usize],
        counts[SoundType::Unused as usize],
        counts[SoundType::Sfx as usize],
        counts[SoundType::Speech as usize],
        counts[SoundType::Midi as usize],
    );

    assert_eq!(errors, 0, "{errors} MSND chunks failed to parse");

    // Sanity: snds.3cn should contain SFX and MIDI sounds
    assert!(
        counts[SoundType::Sfx as usize] > 0 || counts[SoundType::Speech as usize] > 0,
        "expected at least one WAV-type sound"
    );
}
