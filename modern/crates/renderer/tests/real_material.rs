//! Integration test: BMTL → GpuMaterial pipeline from real content files.

use std::fs::File;
use std::io::BufReader;

use chunky_format::ChunkyFile;
use engine::material::BrMaterial;
use engine::tag::CTG_MTRL;
use renderer::convert::material_to_gpu;

const MTRLS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../content-files/mtrls.3cn"
);

#[test]
fn convert_all_bmtl_to_gpu() {
    let file = File::open(MTRLS_PATH).unwrap_or_else(|e| panic!("Cannot open {MTRLS_PATH}: {e}"));
    let cfl = ChunkyFile::read(&mut BufReader::new(file)).unwrap();

    let mut converted = 0u32;

    for entry in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_MTRL) {
        let data = match cfl.get_chunk_data(entry.id.ctg, entry.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let mat = match BrMaterial::from_bytes(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let gpu_mat = material_to_gpu(&mat);

        // Base color channels in [0, 1]
        for &c in &gpu_mat.gpu.base_color {
            assert!(
                (0.0..=1.0).contains(&c),
                "base_color component out of range: {c}"
            );
        }
        // Alpha always 1.0
        assert_eq!(gpu_mat.gpu.base_color[3], 1.0);
        // Coefficients in [0, 1]
        assert!(
            (0.0..=1.0).contains(&gpu_mat.gpu.ambient),
            "ambient out of range"
        );
        assert!(
            (0.0..=1.0).contains(&gpu_mat.gpu.diffuse),
            "diffuse out of range"
        );
        assert!(
            (0.0..=1.0).contains(&gpu_mat.gpu.specular),
            "specular out of range"
        );
        // Power non-negative
        assert!(gpu_mat.gpu.specular_power >= 0.0, "specular_power negative");

        converted += 1;
    }

    assert!(
        converted > 40,
        "Only converted {converted} MTRL materials, expected >40"
    );
}
