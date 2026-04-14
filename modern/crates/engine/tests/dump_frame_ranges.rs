/// Diagnostic: dump nfrm_first / nfrm_last for every actor in every scene of jungle.3mm.
/// Run with: cargo test -p engine --test dump_frame_ranges -- --nocapture

use std::io::BufReader;
use chunky_format::ChunkyFile;
use engine::actor::ActorOnFile;
use engine::scene::SceneHeader;
use engine::tag::{CTG_ACTR, CTG_SCEN};

#[test]
fn dump_frame_ranges() {
    let path = "/Users/devuser/www/3DMMEx/samples/jungle.3mm";
    let f = std::fs::File::open(path).unwrap();
    let mut r = BufReader::new(f);
    let cfl = ChunkyFile::read(&mut r).unwrap();

    let scen_chunks: Vec<_> = cfl.chunks.iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .collect();

    println!("\njungle.3mm — {} scenes", scen_chunks.len());
    println!("{:-<70}", "");

    for (si, scen) in scen_chunks.iter().enumerate() {
        // Parse scene header for nfrm_mac
        let nfrm_mac = match cfl.get_chunk_data(scen.id.ctg, scen.id.cno) {
            Ok(data) if data.len() >= SceneHeader::SIZE => {
                let arr: [u8; 16] = data[..16].try_into().unwrap();
                SceneHeader::from_bytes(&arr).map(|h| h.nfrm_mac).unwrap_or(-1)
            }
            _ => -1,
        };

        let actr_children: Vec<_> = scen.children.iter()
            .filter(|c| c.id.ctg == CTG_ACTR)
            .collect();

        println!("Scene {} (cno={}) — nfrm_mac={} — {} actors",
            si + 1, scen.id.cno, nfrm_mac, actr_children.len());

        for actr in &actr_children {
            let data = match cfl.get_chunk_data(actr.id.ctg, actr.id.cno) {
                Ok(d) => d,
                Err(e) => { println!("  ACTR:{} err: {e:?}", actr.id.cno); continue; }
            };
            if data.len() < ActorOnFile::SIZE {
                println!("  ACTR:{} too short ({})", actr.id.cno, data.len());
                continue;
            }
            let arr: [u8; 44] = data[..44].try_into().unwrap();
            match ActorOnFile::from_bytes(&arr) {
                Err(e) => println!("  ACTR:{} parse err: {e:?}", actr.id.cno),
                Ok(a) => {
                    let has_range = a.nfrm_last > a.nfrm_first;
                    println!("  ACTR:{} arid={} nfrm_first={} nfrm_last={} has_range={}  cno={}",
                        actr.id.cno, a.arid, a.nfrm_first, a.nfrm_last, has_range,
                        a.tag_tmpl.cno);
                }
            }
        }
        println!();
    }
}
