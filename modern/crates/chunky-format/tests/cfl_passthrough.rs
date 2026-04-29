//! Phase 7-spike: passthrough round-trip test.
//!
//! For each of the 22 real .3mm files: read → to_bytes_passthrough() → parse again.
//!
//! Acceptance criteria:
//!   - Re-parse must succeed (no errors)
//!   - Same chunk count
//!   - Each chunk: same ctg/cno, same cb, same flags, same child list
//!   - Byte diff is reported but does NOT fail the test (cosmetic diffs acceptable)
//!
//! The test prints a per-file summary. Human validation (3DMM 1995) is a separate step.

use chunky_format::cfl::ChunkyFile;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // tests run from workspace root (modern/)
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // modern/
        .parent()
        .unwrap() // 3DMMEx/
        .to_path_buf()
}

fn collect_test_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in &["samples", "projects"] {
        let dir_path = root.join(dir);
        if let Ok(rd) = std::fs::read_dir(&dir_path) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "3mm").unwrap_or(false) {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    files
}

struct FileResult {
    path: PathBuf,
    original_len: usize,
    output_len: usize,
    chunk_count: usize,
    parse_ok: bool,
    semantic_ok: bool,
    semantic_errors: Vec<String>,
    byte_identical: bool,
    first_diff_offset: Option<usize>,
}

fn test_file(path: &Path) -> FileResult {
    let original_bytes = std::fs::read(path).expect("read original");
    let original_len = original_bytes.len();

    // Parse
    let mut cursor = std::io::Cursor::new(&original_bytes);
    let cfl = match ChunkyFile::read(&mut cursor) {
        Ok(f) => f,
        Err(e) => {
            return FileResult {
                path: path.to_path_buf(),
                original_len,
                output_len: 0,
                chunk_count: 0,
                parse_ok: false,
                semantic_ok: false,
                semantic_errors: vec![format!("parse failed: {e}")],
                byte_identical: false,
                first_diff_offset: None,
            };
        }
    };
    let chunk_count = cfl.chunks.len();

    // Passthrough
    let output = match cfl.to_bytes_passthrough() {
        Ok(b) => b,
        Err(e) => {
            return FileResult {
                path: path.to_path_buf(),
                original_len,
                output_len: 0,
                chunk_count,
                parse_ok: true,
                semantic_ok: false,
                semantic_errors: vec![format!("to_bytes_passthrough failed: {e}")],
                byte_identical: false,
                first_diff_offset: None,
            };
        }
    };
    let output_len = output.len();

    // Re-parse the output
    let mut cursor2 = std::io::Cursor::new(&output);
    let cfl2 = match ChunkyFile::read(&mut cursor2) {
        Ok(f) => f,
        Err(e) => {
            return FileResult {
                path: path.to_path_buf(),
                original_len,
                output_len,
                chunk_count,
                parse_ok: true,
                semantic_ok: false,
                semantic_errors: vec![format!("re-parse failed: {e}")],
                byte_identical: false,
                first_diff_offset: None,
            };
        }
    };

    // Semantic comparison
    let mut semantic_errors = Vec::new();
    if cfl2.chunks.len() != chunk_count {
        semantic_errors.push(format!(
            "chunk count mismatch: original={} output={}",
            chunk_count,
            cfl2.chunks.len()
        ));
    }
    for orig in &cfl.chunks {
        match cfl2
            .chunks
            .iter()
            .find(|c| c.id.ctg == orig.id.ctg && c.id.cno == orig.id.cno)
        {
            None => {
                semantic_errors.push(format!("missing chunk {:?}:{}", orig.id.ctg, orig.id.cno))
            }
            Some(out_chunk) => {
                if out_chunk.cb != orig.cb {
                    semantic_errors.push(format!(
                        "chunk {:?}:{} cb mismatch: {} vs {}",
                        orig.id.ctg, orig.id.cno, orig.cb, out_chunk.cb
                    ));
                }
                if out_chunk.flags != orig.flags {
                    semantic_errors.push(format!(
                        "chunk {:?}:{} flags mismatch: {:?} vs {:?}",
                        orig.id.ctg, orig.id.cno, orig.flags, out_chunk.flags
                    ));
                }
                if out_chunk.children.len() != orig.children.len() {
                    semantic_errors.push(format!(
                        "chunk {:?}:{} child count: {} vs {}",
                        orig.id.ctg,
                        orig.id.cno,
                        orig.children.len(),
                        out_chunk.children.len()
                    ));
                }
                // Verify chunk data round-trips
                let orig_data = cfl
                    .get_chunk_data(orig.id.ctg, orig.id.cno)
                    .unwrap_or_default();
                let out_data = cfl2
                    .get_chunk_data(out_chunk.id.ctg, out_chunk.id.cno)
                    .unwrap_or_default();
                if orig_data != out_data {
                    semantic_errors.push(format!(
                        "chunk {:?}:{} data mismatch (orig={} out={})",
                        orig.id.ctg,
                        orig.id.cno,
                        orig_data.len(),
                        out_data.len()
                    ));
                }
            }
        }
    }

    // Byte diff (informational only)
    let byte_identical = original_bytes == output;
    let first_diff_offset = if byte_identical {
        None
    } else {
        original_bytes
            .iter()
            .zip(output.iter())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
    };

    FileResult {
        path: path.to_path_buf(),
        original_len,
        output_len,
        chunk_count,
        parse_ok: true,
        semantic_ok: semantic_errors.is_empty(),
        semantic_errors,
        byte_identical,
        first_diff_offset,
    }
}

#[test]
fn passthrough_all_3mm_files() {
    let files = collect_test_files();
    assert!(!files.is_empty(), "No .3mm files found — check repo layout");

    println!("\n{}", "=".repeat(72));
    println!(
        "Phase 7-spike: Passthrough round-trip — {} files",
        files.len()
    );
    println!("{}", "=".repeat(72));

    let mut pass_semantic = 0usize;
    let mut fail_semantic = 0usize;
    let mut byte_identical_count = 0usize;
    let mut failed_files: Vec<String> = Vec::new();

    for path in &files {
        let result = test_file(path);
        let fname = path.file_name().unwrap().to_string_lossy();

        let status = if !result.parse_ok || !result.semantic_ok {
            "FAIL"
        } else if result.byte_identical {
            "BYTE-IDENTICAL"
        } else {
            "SEMANTIC-OK"
        };

        print!(
            "{:<20} {:>14}  chunks={:>4}  orig={:>8}  out={:>8}",
            fname, status, result.chunk_count, result.original_len, result.output_len
        );

        if !result.byte_identical {
            if let Some(off) = result.first_diff_offset {
                print!("  first_diff=0x{:x}", off);
            }
        }
        println!();

        if result.semantic_ok {
            pass_semantic += 1;
            if result.byte_identical {
                byte_identical_count += 1;
            }
        } else {
            fail_semantic += 1;
            failed_files.push(fname.to_string());
            for err in &result.semantic_errors {
                println!("    ERROR: {}", err);
            }
        }
    }

    println!("{}", "=".repeat(72));
    println!(
        "Semantic OK: {}/{}   Byte-identical: {}/{}   FAIL: {}",
        pass_semantic,
        files.len(),
        byte_identical_count,
        files.len(),
        fail_semantic
    );

    // Hard assertion: all files must pass semantic check
    assert!(
        fail_semantic == 0,
        "Semantic failures in: {:?}",
        failed_files
    );
}
