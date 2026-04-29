use chunky_format::ChunkyFile;
use engine::tag::CTG_TMAP;
use engine::tmap::BrTmap;
use std::fs::File;
use std::io::BufReader;

fn main() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../content-files/tmpls.3cn"
    );
    let cfl = ChunkyFile::read(&mut BufReader::new(File::open(path).unwrap())).unwrap();

    let mut ok_count = 0u32;
    let mut err_count = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_TMAP).take(5) {
        println!(
            "TMAP {}: size={} packed={}",
            entry.id,
            entry.cb,
            entry.is_packed()
        );

        // Access raw (possibly compressed) bytes directly
        let raw: &Vec<u8> = match cfl.chunk_data.get(&(entry.id.ctg, entry.id.cno)) {
            Some(r) => r,
            None => {
                println!("  not in chunk_data map");
                err_count += 1;
                continue;
            }
        };
        println!("  raw len={}", raw.len());
        if raw.len() >= 8 {
            let fmt = u32::from_be_bytes(raw[0..4].try_into().unwrap());
            let cb_dst = u32::from_be_bytes(raw[4..8].try_into().unwrap());
            let fmt_str: String = fmt.to_be_bytes().iter().map(|&b| b as char).collect();
            println!(
                "  codec header: fmt=0x{:08X} ('{}') cb_dst={}",
                fmt, fmt_str, cb_dst
            );
        }
        println!("  first 16 raw bytes: {:02X?}", &raw[..raw.len().min(16)]);

        match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(data) => {
                println!("  decompressed len={}", data.len());
                println!("  first 24 bytes: {:02X?}", &data[..data.len().min(24)]);
                match BrTmap::from_bytes(&data) {
                    Ok(t) => {
                        println!(
                            "  parsed: {}x{} type={} row_bytes={}",
                            t.width, t.height, t.pixel_type, t.row_bytes
                        );
                        ok_count += 1;
                    }
                    Err(e) => {
                        println!("  parse FAILED: {e}");
                        err_count += 1;
                    }
                }
            }
            Err(e) => {
                println!("  decompress FAILED: {e}");
                err_count += 1;
            }
        }
    }
    println!("\nok={ok_count} err={err_count}");
}
