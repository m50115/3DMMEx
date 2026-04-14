//! Diagnostic: print hex bytes of first WAVE chunk to understand format.
//! Run with: cargo test -p audio --test probe_wave_format -- --nocapture

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::tag::CTG_WAVE;

const SNDS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/snds.3cn");

#[test]
fn probe_first_wave_bytes() {
    let file = File::open(SNDS_PATH).unwrap();
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).unwrap();

    for chunk in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_WAVE).take(2) {
        let data = cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno).unwrap();
        println!("\nWAVE cno={} total_bytes={}", chunk.id.cno, data.len());

        // hex dump first 80 bytes
        print!("hex: ");
        for (i, b) in data.iter().take(80).enumerate() {
            if i > 0 && i % 16 == 0 { print!("\n     "); }
            print!("{:02X} ", b);
        }
        println!();

        // ASCII of first 16 bytes
        print!("ascii: ");
        for b in data.iter().take(16) {
            print!("{}", if b.is_ascii_graphic() { *b as char } else { '.' });
        }
        println!();

        // Parse RIFF structure manually
        if data.len() >= 12 && &data[0..4] == b"RIFF" {
            let riff_size = u32::from_le_bytes(data[4..8].try_into().unwrap());
            println!("RIFF size field = {riff_size}, actual remaining = {}", data.len() - 8);
            println!("WAVE magic: {:?}", &data[8..12]);

            let mut offset = 12usize;
            while offset + 8 <= data.len() {
                let tag = &data[offset..offset+4];
                let chunk_size = u32::from_le_bytes(data[offset+4..offset+8].try_into().unwrap()) as usize;
                print!("  chunk '{}' size={}", String::from_utf8_lossy(tag), chunk_size);
                if tag == b"fmt " && offset + 8 + chunk_size.min(26) <= data.len() {
                    let fmt = &data[offset+8..offset+8+chunk_size.min(26)];
                    let audio_fmt  = u16::from_le_bytes(fmt[0..2].try_into().unwrap());
                    let channels   = u16::from_le_bytes(fmt[2..4].try_into().unwrap());
                    let srate      = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
                    let byte_rate  = u32::from_le_bytes(fmt[8..12].try_into().unwrap());
                    let block_align= u16::from_le_bytes(fmt[12..14].try_into().unwrap());
                    let bits       = u16::from_le_bytes(fmt[14..16].try_into().unwrap());
                    print!(" → fmt={audio_fmt} ch={channels} srate={srate} brate={byte_rate} \
                            align={block_align} bits={bits}");
                }
                println!();
                offset += 8 + chunk_size + (chunk_size % 2); // RIFF word-aligns chunks
            }
        } else {
            println!("  NOT a RIFF file");
        }
    }
}
