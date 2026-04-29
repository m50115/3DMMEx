//! Smoke test for cached scene rendering with a bound texture key.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::model::Model;
use engine::tag::{CTG_BMDL, CTG_TMAP};
use glam::Mat4;
use renderer::headless::HeadlessRenderer;

const TMPLS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/tmpls.3cn"
);

#[test]
fn render_scene_with_cached_texture_key() {
    let Some(mut renderer) = HeadlessRenderer::try_new() else {
        println!("render_scene_with_cached_texture_key: no GPU adapter - skipping");
        return;
    };

    let file = File::open(TMPLS_PATH).unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).expect("parse tmpls.3cn");

    let (bmdl_key, model) = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_BMDL)
        .find_map(|c| {
            let data = cfl.get_chunk_data(c.id.ctg, c.id.cno).ok()?;
            if data.len() < 80 {
                return None;
            }
            let m = Model::from_bytes(&data).ok()?;
            if m.has_valid_faces() && !m.vertices.is_empty() {
                Some(((c.id.ctg, c.id.cno), m))
            } else {
                None
            }
        })
        .expect("no renderable model in tmpls.3cn");

    renderer.ensure_mesh(bmdl_key, &model);

    let texture_key = (CTG_TMAP, 0xFFFF_FFFE);
    let rgba = [
        255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    renderer.ensure_texture(texture_key, 2, 2, &rgba);
    assert!(renderer.has_texture(texture_key));

    let out = renderer.render_scene(
        &[(bmdl_key, Mat4::IDENTITY, Some(texture_key))],
        64,
        64,
        None,
    );
    assert_eq!(out.len(), 64 * 64 * 4);
}
