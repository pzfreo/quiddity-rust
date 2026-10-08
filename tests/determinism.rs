//! Output does not depend on hash order: two corpus parts whose reading and recognition go through
//! `HashMap`s (the STEP reader's entity indices; circular face patterns on cgb203, thin walls on
//! cgb241; the correspondence's face index) are read, recognised and corresponded with themselves
//! twice in one process, where each map gets a fresh random hash seed, and the JSON the CLI would
//! print must be the same byte for byte.

mod common;

use quiddity::correspondence;

const PARTS: [&str; 2] = [
    "cadgenbench_inputs/cgb203.step",
    "cadgenbench_inputs/cgb241.step",
];

/// What `quiddity <file.step>` prints, then what `quiddity correspond <file> <file>` prints.
fn outputs(path: &std::path::Path) -> (String, String) {
    let part = quiddity::read_step_file(path).unwrap();
    let recognition = correspondence::recognise(&part);
    let c =
        correspondence::correspond(&recognition.fingerprints, &recognition.fingerprints).unwrap();
    (
        serde_json::to_string_pretty(&recognition).unwrap(),
        serde_json::to_string_pretty(&c).unwrap(),
    )
}

#[test]
fn recognition_json_is_the_same_twice_in_one_process() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    for name in PARTS {
        let path = dir.join(name);
        let first = outputs(&path);
        let second = outputs(&path);
        assert!(
            first.0 == second.0,
            "{name}: recognition JSON differs between runs"
        );
        assert!(
            first.1 == second.1,
            "{name}: correspondence JSON differs between runs"
        );
        // The parts exercise the maps they were chosen for.
        let family = if name.ends_with("cgb203.step") {
            "circular_face_patterns"
        } else {
            "thin_wall_bodies"
        };
        let json: serde_json::Value = serde_json::from_str(&first.0).unwrap();
        assert!(
            json[family].as_array().is_some_and(|a| !a.is_empty()),
            "{name} no longer finds {family}"
        );
    }
}
