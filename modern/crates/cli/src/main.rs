//! CLI tools for inspecting and validating 3D Movie Maker files.

use std::path::PathBuf;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: cli <command> <file.3mm>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  inspect    Show chunk tree of a chunky file");
        eprintln!("  validate   Verify file structure consistency");
        std::process::exit(1);
    }

    let command = &args[1];
    let path = PathBuf::from(&args[2]);

    match command.as_str() {
        "inspect" => inspect(&path),
        "validate" => validate(&path),
        _ => {
            eprintln!("Unknown command: {}", command);
            std::process::exit(1);
        }
    }
}

fn inspect(path: &PathBuf) {
    use chunky_format::ChunkyFile;
    use std::io::BufReader;

    let file = std::fs::File::open(path).expect("Failed to open file");
    let mut reader = BufReader::new(file);

    match ChunkyFile::read(&mut reader) {
        Ok(cfl) => {
            println!("=== Chunky File: {} ===", path.display());
            println!("Creator: 0x{:08X}", cfl.header.ctg_creator);
            println!("Version: {}/{}", cfl.header.version_current, cfl.header.version_back);
            println!("Chunks: {}", cfl.chunks.len());
            println!();

            for chunk in &cfl.chunks {
                let packed = if chunk.is_packed() { " [PACKED]" } else { "" };
                let forest = if chunk.is_forest() { " [FOREST]" } else { "" };
                let name = chunk.name.as_deref().unwrap_or("");
                println!(
                    "  {} size={} children={} refs={}{}{} {}",
                    chunk.id, chunk.cb, chunk.child_count, chunk.ref_count,
                    packed, forest, name
                );

                for child in &chunk.children {
                    println!("    └── {} (chid={})", child.id, child.chid);
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    }
}

fn validate(path: &PathBuf) {
    use chunky_format::ChunkyFile;
    use std::io::{BufReader, Read};

    // Read raw bytes for consistency checking
    let raw_bytes = std::fs::read(path).expect("Failed to read file");

    let file = std::fs::File::open(path).expect("Failed to open file");
    let mut reader = BufReader::new(file);

    let cfl = match ChunkyFile::read(&mut reader) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FAIL: {}", e);
            std::process::exit(1);
        }
    };

    let file_size = raw_bytes.len() as u32;
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // ── Header sanity ────────────────────────────────────────────────────────
    if cfl.header.fp_mac != file_size {
        warnings.push(format!(
            "fp_mac ({}) != actual file size ({})", cfl.header.fp_mac, file_size
        ));
    }

    // ── Index bytes match ────────────────────────────────────────────────────
    let idx_start = cfl.header.fp_index as usize;
    let idx_end   = idx_start + cfl.header.cb_index as usize;
    if idx_end > raw_bytes.len() {
        errors.push("index region extends beyond file".into());
    } else if &raw_bytes[idx_start..idx_end] != cfl.raw_index.as_slice() {
        errors.push("raw_index does not match file bytes at fp_index".into());
    }

    // ── Chunk data bounds and content ────────────────────────────────────────
    let mut covered: Vec<(u32, u32)> = Vec::new(); // (fp, fp+cb) ranges

    for chunk in &cfl.chunks {
        if chunk.cb == 0 { continue; }

        let fp = chunk.fp;
        let cb = chunk.cb;
        let end = fp.saturating_add(cb);

        // Within file bounds
        if end as usize > raw_bytes.len() {
            errors.push(format!(
                "{}: fp={} cb={} extends beyond file ({})", chunk.id, fp, cb, file_size
            ));
            continue;
        }

        // Chunk data in chunk_data matches raw file bytes
        if let Some(stored) = cfl.chunk_data.get(&(chunk.id.ctg, chunk.id.cno)) {
            let file_slice = &raw_bytes[fp as usize..end as usize];
            if stored.as_slice() != file_slice {
                errors.push(format!("{}: stored data does not match file bytes", chunk.id));
            }
        } else {
            errors.push(format!("{}: missing from chunk_data map", chunk.id));
        }

        // Overlap detection
        for &(other_fp, other_end) in &covered {
            if fp < other_end && end > other_fp {
                errors.push(format!(
                    "{}: overlaps with chunk at fp={}", chunk.id, other_fp
                ));
            }
        }
        covered.push((fp, end));

        // Children reference valid chunks
        for child in &chunk.children {
            if cfl.find_chunk(child.id.ctg, child.id.cno).is_none() {
                warnings.push(format!(
                    "{}: child {} not found in index", chunk.id, child.id
                ));
            }
        }
    }

    // ── Report ───────────────────────────────────────────────────────────────
    println!("=== Validate: {} ===", path.display());
    println!("  Chunks:    {}", cfl.chunks.len());
    println!("  File size: {} bytes", file_size);
    println!("  Index:     fp={} cb={}", cfl.header.fp_index, cfl.header.cb_index);

    for w in &warnings {
        println!("  WARN: {}", w);
    }

    if errors.is_empty() {
        println!("  RESULT: PASS");
    } else {
        for e in &errors {
            println!("  ERROR: {}", e);
        }
        println!("  RESULT: FAIL ({} errors)", errors.len());
        std::process::exit(1);
    }
}
