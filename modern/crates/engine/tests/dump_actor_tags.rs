use chunky_format::ChunkyFile;
use engine::actor::ActorOnFile;
use engine::model::Model;
use engine::tag::{CTG_ACTR, CTG_BMDL, CTG_TMPL};
/// Diagnostic test: dump tag_tmpl from every ACTR in sample .3mm files,
/// then verify the BMDL lookup in tmpls.3cn.
/// Run with: cargo test -p engine --test dump_actor_tags -- --nocapture
use std::io::BufReader;

#[test]
fn dump_actor_tags() {
    let tmpls_path = "/Users/devuser/www/3DMMEx/content-files/tmpls.3cn";
    let tf = std::fs::File::open(tmpls_path).unwrap();
    let mut tr = BufReader::new(tf);
    let tmpls = ChunkyFile::read(&mut tr).unwrap();

    let samples = [
        "/Users/devuser/www/3DMMEx/samples/bongo.3mm",
        "/Users/devuser/www/3DMMEx/samples/jungle.3mm",
    ];

    for path in &samples {
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let f = std::fs::File::open(path).unwrap();
        let mut r = BufReader::new(f);
        let cfl = ChunkyFile::read(&mut r).unwrap();

        for chunk in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_ACTR) {
            let data = match cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno) {
                Ok(d) => d,
                Err(e) => {
                    println!("{name} ACTR:{} get_data err: {e:?}", chunk.id.cno);
                    continue;
                }
            };
            if data.len() < 44 {
                println!("{name} ACTR:{} too short {}", chunk.id.cno, data.len());
                continue;
            }
            let arr: [u8; 44] = data[..44].try_into().unwrap();
            let actf = match ActorOnFile::from_bytes(&arr) {
                Ok(a) => a,
                Err(e) => {
                    println!("{name} ACTR:{} parse err: {e:?}", chunk.id.cno);
                    continue;
                }
            };

            let tag = actf.tag_tmpl;
            let ctg_str: String = tag
                .ctg
                .to_be_bytes()
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            print!(
                "{name} ACTR:{} sid={} ctg='{}' cno={} → ",
                chunk.id.cno, tag.sid, ctg_str, tag.cno
            );

            if tag.ctg != CTG_TMPL {
                println!("NOT TMPL, skip");
                continue;
            }

            // Find TMPL in tmpls.3cn
            let tmpl = match tmpls
                .chunks
                .iter()
                .find(|c| c.id.ctg == CTG_TMPL && c.id.cno == tag.cno)
            {
                Some(t) => t,
                None => {
                    println!("TMPL:{} NOT FOUND in tmpls.3cn", tag.cno);
                    continue;
                }
            };

            // Find BMDL children
            let bmdl_children: Vec<_> = tmpl
                .children
                .iter()
                .filter(|c| c.id.ctg == CTG_BMDL)
                .collect();
            if bmdl_children.is_empty() {
                println!("TMPL:{} has no BMDL children", tag.cno);
                continue;
            }

            let first = bmdl_children[0];
            match tmpls.get_chunk_data(first.id.ctg, first.id.cno) {
                Err(e) => println!("BMDL:{} get_data err: {e:?}", first.id.cno),
                Ok(data) => match Model::from_bytes(&data) {
                    Err(e) => println!("BMDL:{} parse err: {e:?}", first.id.cno),
                    Ok(m) => println!(
                        "BMDL:{} verts={} faces={} valid={} non_empty={}",
                        first.id.cno,
                        m.vertices.len(),
                        m.faces.len(),
                        m.has_valid_faces(),
                        !m.vertices.is_empty()
                    ),
                },
            }
        }
    }
}
