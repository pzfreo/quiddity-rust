//! Export face integrals for tools/check_face_integrals.py; one JSON line per input file.
use haecceity::step::{IDENTITY, read_step_file_placed};
use serde_json::json;

fn main() {
    for file in std::env::args().skip(1) {
        let part =
            read_step_file_placed(std::path::Path::new(&file), &IDENTITY).expect("read STEP");
        let faces: Vec<_> = (0..part.faces.len())
            .map(|f| {
                part.face_moments(f).map(|m| {
                    json!({
                        "area": m.area, "centroid": m.centroid,
                        "second_moments": m.second_moments,
                        "area_vector": m.area_vector,
                        "mass": part.face_mass(f),
                        "bounds": [part.face_bounds(f).min, part.face_bounds(f).max],
                        "entity": part.face_source(f).map(|s| s.entity),
                    })
                })
            })
            .collect();
        println!("{}", json!({"file": file, "faces": faces}));
    }
}
