//! Temporary probe: dump TMPL chunk subtree to diagnose CMTL/MTRL/TMAP nesting.
//! Delete after Ph10.4 web texture fix lands.

use chunky_format::ChunkyFile;
use engine::tag::*;
use std::fs::File;
use std::io::BufReader;

fn ctg_name(c: u32) -> String {
    String::from_utf8_lossy(&c.to_be_bytes()).into_owned()
}

fn dump_subtree(cfl: &ChunkyFile, ctg: u32, cno: u32, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    let prefix = "    ".repeat(depth);
    let chunk = match cfl
        .chunks
        .iter()
        .find(|c| c.id.ctg == ctg && c.id.cno == cno)
    {
        Some(c) => c,
        None => {
            println!("{}{} cno={} (chunk body not found)", prefix, ctg_name(ctg), cno);
            return;
        }
    };
    println!(
        "{}{} cno={} cb={} children={}",
        prefix,
        ctg_name(ctg),
        cno,
        chunk.cb,
        chunk.children.len()
    );
    for ch in &chunk.children {
        let p2 = "    ".repeat(depth + 1);
        println!(
            "{}└─ {} cno={} chid={}",
            p2,
            ctg_name(ch.id.ctg),
            ch.id.cno,
            ch.chid
        );
        dump_subtree(cfl, ch.id.ctg, ch.id.cno, depth + 2, max_depth);
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("content-files/tmpls.3cn"));
    let f = File::open(&path).unwrap_or_else(|e| panic!("open {}: {}", path, e));
    let cfl = ChunkyFile::read(&mut BufReader::new(f)).expect("parse");

    println!("File: {} — {} chunks total", path, cfl.chunks.len());

    let mut by_ctg = std::collections::BTreeMap::new();
    for c in &cfl.chunks {
        *by_ctg.entry(ctg_name(c.id.ctg)).or_insert(0u32) += 1;
    }
    println!("Top-level counts:");
    for (k, v) in &by_ctg {
        println!("  {} = {}", k, v);
    }

    println!("\n=== First 3 TMPL subtrees (depth ≤ 3) ===");
    for (i, c) in cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_TMPL)
        .take(3)
        .enumerate()
    {
        println!("\n[{}]", i);
        dump_subtree(&cfl, CTG_TMPL, c.id.cno, 0, 3);
    }
}
