//! One distinct part of a file as a `Part` in its own coordinates (`step::read_part`,
//! `StepFile::part`), the parts' display names and the assembly structure (`StepFile`).
//!
//! The parts' own geometry is checked against the whole file's read: each placement of a part
//! is its own `Part` moved by that placement (face by face: the same file face, the same area,
//! the centroid moved), with every face and edge source kept. Names are OpenCascade XCAF's as
//! specify-core reports them; `tests/face_sources.rs` checks them over the corpus and the NIST
//! files against specify-core's capture, and `wrapped.step` here (made after that capture) was
//! checked with specify-core's `load.parts` (OpenCascade 7.9): `['Gehäuse Ø']`.

mod common;

use std::io::Read;
use std::path::{Path, PathBuf};

use haecceity::Part;
use haecceity::step::{
    Assembly, Component, PartDefinition, Placement, StepFile, read_part, read_part_definitions,
    read_step,
};

fn bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

fn apply(p: &Placement, x: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| (0..3).map(|k| p[i][k] * x[k]).sum::<f64>() + p[i][3])
}

/// `own` is `def` in its own coordinates: its faces are `def.faces` in order with their
/// sources kept, its edges are `def.edges` (as a set) with theirs, and each placement of `def`
/// in the whole file's read (`placed`) is `own` moved by that placement: per face the same
/// area (1e-9 relative) and the centroid moved (1e-6 mm), and the same unresolved faces.
fn check_own(name: &str, def: &PartDefinition, own: &Part, placed: &Part) {
    assert_eq!(own.faces.len(), def.faces.len(), "{name}: face count");
    for (i, &entity) in def.faces.iter().enumerate() {
        let s = own.face_source(i).expect("face source");
        assert_eq!(s.entity, entity, "{name}: face {i}");
        assert!(
            def.placements[0].instances.contains(&s.instance),
            "{name}: face {i} is of instance {}, not of the first placement",
            s.instance
        );
    }
    let mut edges: Vec<u64> = (0..own.edges.len())
        .map(|e| own.edge_source(e).expect("edge source").entity)
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut want = def.edges.clone();
    want.sort_unstable();
    assert_eq!(edges, want, "{name}: edges");
    for placement in &def.placements {
        let faces: Vec<usize> = (0..placed.faces.len())
            .filter(|&f| {
                placed
                    .face_source(f)
                    .is_some_and(|s| placement.instances.contains(&s.instance))
            })
            .collect();
        assert_eq!(faces.len(), own.faces.len(), "{name}: placed face count");
        for (i, &f) in faces.iter().enumerate() {
            assert_eq!(
                placed.face_source(f).unwrap().entity,
                def.faces[i],
                "{name}: placed face {f}"
            );
            assert_eq!(
                placed.unresolved_faces().contains(&f),
                own.unresolved_faces().contains(&i),
                "{name}: face {i} unresolved"
            );
            match (own.face_moments(i), placed.face_moments(f)) {
                (Some(a), Some(b)) => {
                    assert!(
                        (a.area - b.area).abs() <= 1e-9 * b.area.abs().max(1.0),
                        "{name}: face {i} area {} vs placed {}",
                        a.area,
                        b.area
                    );
                    let moved = apply(&placement.placement, a.centroid);
                    let at = b.centroid;
                    for k in 0..3 {
                        assert!(
                            (moved[k] - at[k]).abs() < 1e-6,
                            "{name}: face {i} centroid {moved:?} vs placed {at:?}"
                        );
                    }
                }
                (None, None) => {}
                (a, b) => panic!("{name}: face {i} moments {a:?} vs placed {b:?}"),
            }
        }
    }
}

/// The AP242 assembly fixture: a plate and a pin placed twice (once turned 37° about an
/// oblique axis), one assembly holding the three occurrences in file order.
#[test]
fn an_assembly_part_is_read_in_its_own_coordinates() {
    let bytes = bytes(&common::fixtures().join("ap242/assembly/assembly.step"));
    let file = StepFile::read(&bytes).unwrap();
    let names: Vec<&str> = file.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["plate", "pin"]);
    assert_eq!(
        file.assemblies,
        [Assembly {
            product_definition: 5,
            name: "assembly".into(),
            components: vec![
                Component {
                    occurrence: 968,
                    definition: 35
                },
                Component {
                    occurrence: 1265,
                    definition: 972
                },
                Component {
                    occurrence: 1271,
                    definition: 972
                },
            ],
        }]
    );
    let pin = &file.parts[1];
    assert_eq!(pin.placements.len(), 2);
    assert!(
        pin.placements
            .iter()
            .all(|p| p.placement != haecceity::step::IDENTITY)
    );
    let placed = read_step(&bytes).unwrap();
    for (index, def) in file.parts.iter().enumerate() {
        let own = read_part(&bytes, def).unwrap();
        check_own(&def.name, def, &own, &placed);
        // The file read once gives the same part.
        let again = file.part(index).unwrap();
        assert_eq!(again.faces.len(), own.faces.len());
        assert_eq!(again.edges.len(), own.edges.len());
        for f in 0..own.faces.len() {
            assert_eq!(again.face_source(f), own.face_source(f));
            assert_eq!(
                again.face_moments(f).map(|m| m.area),
                own.face_moments(f).map(|m| m.area)
            );
        }
    }
    assert_eq!(
        read_part_definitions(&bytes)
            .unwrap()
            .iter()
            .map(|d| (d.product_definition, d.name.clone(), d.faces.clone()))
            .collect::<Vec<_>>(),
        file.parts
            .iter()
            .map(|d| (d.product_definition, d.name.clone(), d.faces.clone()))
            .collect::<Vec<_>>()
    );
}

/// A part held alone by a wrapper assembly is named after it, escapes decoded (XCAF, through
/// specify-core: 'Gehäuse Ø'); the part itself is read in its own coordinates.
#[test]
fn a_wrapped_part_takes_its_assemblys_name() {
    let bytes = bytes(&common::fixtures().join("ap242/assembly/wrapped.step"));
    let file = StepFile::read(&bytes).unwrap();
    assert_eq!(file.parts.len(), 1);
    assert_eq!(file.parts[0].name, "Gehäuse Ø");
    assert_eq!(file.assemblies.len(), 1);
    assert_eq!(file.assemblies[0].name, "Gehäuse Ø");
    assert_eq!(
        file.assemblies[0].components,
        [Component {
            occurrence: 960,
            definition: file.parts[0].product_definition
        }]
    );
    let def = &file.parts[0];
    assert_ne!(def.placements[0].placement, haecceity::step::IDENTITY);
    let own = file.part(0).unwrap();
    check_own(&def.name, def, &own, &read_step(&bytes).unwrap());
}

/// A definition that is not the file's is refused, naming the part.
#[test]
fn a_part_of_another_file_is_refused() {
    let assembly = bytes(&common::fixtures().join("ap242/assembly/assembly.step"));
    let wrapped = bytes(&common::fixtures().join("ap242/assembly/wrapped.step"));
    let pin = read_part_definitions(&assembly).unwrap().remove(1);
    let err = read_part(&wrapped, &pin).unwrap_err().to_string();
    assert!(err.contains("part #972 (pin)"), "{err}");
    // A definition of the same product definition with other faces.
    let mut plate = read_part_definitions(&wrapped).unwrap().remove(0);
    plate.faces.swap(0, 1);
    let err = read_part(&wrapped, &plate).unwrap_err().to_string();
    assert!(
        err.contains("(Gehäuse Ø)") && err.contains("in their order"),
        "{err}"
    );
    plate.faces.push(1);
    let err = read_part(&wrapped, &plate).unwrap_err().to_string();
    let n = plate.faces.len() - 1;
    assert!(
        err.contains(&format!("face {n} (#1) is read 0 times, not once")),
        "{err}"
    );
}

/// NIST's AP242 test models, each a part stored as an assembly of its representation items in
/// OpenCascade (one product definition here): the part in its own coordinates is the file's
/// read, face for face, sources and unresolved faces kept.
#[test]
fn nist_parts_are_their_files_read() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(common::fixtures().join("ap242/nist"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".stp.gz"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 7);
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = bytes(&path);
        let file = StepFile::read(&bytes).unwrap();
        assert!(file.assemblies.is_empty(), "{name}");
        let placed = read_step(&bytes).unwrap();
        for (index, def) in file.parts.iter().enumerate() {
            let own = file.part(index).unwrap();
            check_own(&name, def, &own, &placed);
            if file.parts.len() == 1 {
                assert_eq!(own.faces.len(), placed.faces.len(), "{name}");
                assert_eq!(own.unresolved_faces(), placed.unresolved_faces(), "{name}");
                assert_eq!(own.unresolved_edges(), placed.unresolved_edges(), "{name}");
            }
        }
    }
}

/// Names as XCAF gives them on the corpus's NIST files: the product's id when its name is
/// empty (FTC-08, CTC-04).
#[test]
fn an_empty_product_name_is_its_id() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none_or(|v| v != "1"),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        return;
    };
    for (file, name) in [
        ("nist/nist_ftc_08_asme1_rc.stp", "NIST PMI FTC 08 ASME1"),
        ("nist/nist_ctc_04_asme1_rd.stp", "NIST PMI CTC 04 ASME1"),
        ("nist/nist_ftc_07_asme1_rd.stp", "nist_ftc_07_asme1"),
    ] {
        let parts = read_part_definitions(&bytes(&dir.join(file))).unwrap();
        assert_eq!(parts.len(), 1, "{file}");
        assert_eq!(parts[0].name, name, "{file}");
    }
}
