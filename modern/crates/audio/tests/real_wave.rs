//! Integration test: decode real WAVE chunks from snds.3cn.
//!
//! Loads the first `CTG_WAVE` chunk from snds.3cn and verifies:
//! - RIFF magic is present
//! - Sample rate = 22050 Hz
//! - Channels = 1 (mono)
//! - Non-zero sample count
//!
//! Also smoke-tests a batch of 100 WAVE chunks to confirm consistent format.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::tag::CTG_WAVE;

use audio::decode_wav;

const SNDS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/snds.3cn"
);

#[test]
fn first_wave_chunk_decodes() {
    let file = File::open(SNDS_PATH).unwrap_or_else(|e| panic!("Cannot open {SNDS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse snds.3cn");

    let wave_chunk = cfl
        .chunks
        .iter()
        .find(|c| c.id.ctg == CTG_WAVE)
        .expect("no WAVE chunks in snds.3cn");

    let data = cfl
        .get_chunk_data(wave_chunk.id.ctg, wave_chunk.id.cno)
        .expect("get WAVE data");

    // Must start with RIFF magic
    assert!(
        data.len() >= 4 && &data[0..4] == b"RIFF",
        "WAVE chunk does not start with RIFF magic (first 4 bytes: {:?})",
        &data[..data.len().min(4)]
    );

    let (info, samples) = decode_wav(&data).expect("decode WAV");

    println!(
        "WAVE cno={}: fmt={} {}Hz {}ch {} samples",
        wave_chunk.id.cno, info.format, info.sample_rate, info.channels, info.sample_count
    );

    assert_eq!(info.sample_rate, 22050, "expected 22050 Hz");
    assert_eq!(info.channels, 1, "expected mono");
    assert_eq!(info.format, 2, "expected MS-ADPCM (fmt=2)");
    assert!(info.sample_count > 0, "decoded 0 samples");
    assert_eq!(samples.len(), info.sample_count);
}

#[test]
fn batch_wave_chunks_decode() {
    let file = File::open(SNDS_PATH).unwrap_or_else(|e| panic!("Cannot open {SNDS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse snds.3cn");

    let wave_chunks: Vec<_> = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_WAVE)
        .take(100)
        .collect();

    assert!(!wave_chunks.is_empty(), "no WAVE chunks found");

    let mut ok = 0usize;
    let mut errors = 0usize;

    for chunk in &wave_chunks {
        let data = match cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno) {
            Ok(d) => d,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        match decode_wav(&data) {
            Ok((info, _)) => {
                assert_eq!(info.sample_rate, 22050);
                assert_eq!(info.channels, 1);
                ok += 1;
            }
            Err(e) => {
                eprintln!("WAVE cno={} decode error: {e}", chunk.id.cno);
                errors += 1;
            }
        }
    }

    println!(
        "batch_wave_chunks_decode: {ok} OK, {errors} errors out of {}",
        wave_chunks.len()
    );

    assert_eq!(errors, 0, "{errors} WAVE chunks failed to decode");
}
