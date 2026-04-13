//! Integration test: full pipeline CFL → Model → Mesh from real content files.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::model::Model;
use engine::tag::CTG_BMDL;
use renderer::convert::model_to_mesh;

const TMPLS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../content-files/tmpls.3cn");

/// Helper: find the first BMDL chunk with valid renderable geometry.
fn find_renderable_model(cfl: &ChunkyFile) -> Option<Model> {
    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < 80 { continue; }

        let model = match Model::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if model.header.vertex_count == 0 { continue; }
        if model.has_valid_faces() {
            return Some(model);
        }
    }
    None
}

#[test]
fn full_pipeline_model_to_mesh() {
    let file = File::open(TMPLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).unwrap();

    let model = find_renderable_model(&cfl).expect("No renderable BMDL found");

    let mesh = model_to_mesh(&model);

    // Vertex count matches
    assert_eq!(mesh.vertices.len(), model.header.vertex_count as usize);

    // Index count = 3 × face count (triangles)
    assert_eq!(mesh.indices.len(), model.header.face_count as usize * 3);

    // All indices in range
    let nv = mesh.vertices.len() as u32;
    for &idx in &mesh.indices {
        assert!(idx < nv, "Index {idx} out of range (nv={nv})");
    }

    // Positions are finite
    for v in &mesh.vertices {
        for &c in &v.position {
            assert!(c.is_finite(), "Non-finite position: {:?}", v.position);
        }
    }

    // Normals are finite
    for v in &mesh.vertices {
        for &c in &v.normal {
            assert!(c.is_finite(), "Non-finite normal: {:?}", v.normal);
        }
    }

    // Colors in [0, 1]
    for v in &mesh.vertices {
        for &c in &v.color {
            assert!((0.0..=1.0).contains(&c), "Color out of range: {:?}", v.color);
        }
    }

    // UVs are finite
    for v in &mesh.vertices {
        for &c in &v.uv {
            assert!(c.is_finite(), "Non-finite UV: {:?}", v.uv);
        }
    }
}

#[test]
fn convert_all_renderable_models() {
    let file = File::open(TMPLS_PATH)
        .unwrap_or_else(|e| panic!("Cannot open {TMPLS_PATH}: {e}"));
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader).unwrap();

    let mut converted = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_BMDL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let model = match Model::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !model.has_valid_faces() { continue; }
        if model.header.vertex_count == 0 { continue; }

        let mesh = model_to_mesh(&model);

        assert_eq!(mesh.vertices.len(), model.vertices.len());
        assert_eq!(mesh.index_count(), model.faces.len() as u32 * 3);
        assert!(mesh.radius >= 0.0);

        converted += 1;
    }

    assert!(
        converted > 150,
        "Only converted {converted} renderable models, expected >150"
    );
}
