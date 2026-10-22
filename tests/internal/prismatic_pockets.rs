//! Prismatic pocket evidence on the parts of Python's own evidence tests
//! (`tests/test_prismatic_pockets.py`, captured as STEP): the captured replay compares records
//! only, so the walls each pocket is defined by and the faces it consults (Python's
//! `constituent` less its defining faces) are checked here, as Python's tests check them.

use std::path::Path;

use quiddity::features::Context;
use quiddity::features::graph::normal;
use quiddity::features::prismatic_pockets::discover_verified;
use quiddity::read_step_file;

/// The one pocket of a captured part: its side count, defining faces and consulted faces.
fn one_pocket(file: &str) -> (usize, Vec<usize>, Vec<usize>, quiddity::Part) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/captured")
        .join(file);
    let part = read_step_file(&path).unwrap();
    let found = discover_verified(&Context::new(&part)).expect("evidence on one valid solid");
    let [pocket] = &found[..] else {
        panic!("{file}: {} pockets", found.len());
    };
    let (sides, defining, context) = (
        pocket.record.sides,
        pocket.defining.clone(),
        pocket.context.clone(),
    );
    (sides, defining, context, part)
}

/// `test_both_cap_orientations_issue_wall_defining_and_floor_constituent_evidence` and
/// `test_each_prismatic_section_retains_its_proved_floor_as_constituent`: a ring pocket is
/// defined by its walls and consults exactly its floor, across the axis.
#[test]
fn a_ring_pocket_is_defined_by_its_walls_and_consults_its_floor() {
    for (file, sides) in [
        ("8e8c441c3860c9de.step.gz", 3), // triangular, open upwards
        ("632c16ed614a6b69.step.gz", 3), // the same mirrored, open downwards
        ("7f97f8ab4443654f.step.gz", 4),
        ("a8f0d0f61d7accd7.step.gz", 6),
    ] {
        let (found, defining, context, part) = one_pocket(file);
        assert_eq!((found, defining.len()), (sides, sides), "{file}");
        let [floor] = context[..] else {
            panic!("{file}: consulted {context:?}");
        };
        assert!(!defining.contains(&floor), "{file}");
        let n = normal(&part, floor).unwrap();
        assert!(
            (n[2].abs() - 1.0).abs() < 1e-9,
            "{file}: floor normal {n:?}"
        );
    }
}

/// `test_partial_mouth_treatment_does_not_hide_a_uniquely_bounded_pocket`: a pocket recovered
/// past a chamfered or filleted mouth edge is still defined by its walls alone, and consults its
/// floor and the treatment face (Python's constituent is the walls and two more).
#[test]
fn a_recovered_pocket_consults_its_floor_and_the_treatment() {
    for (file, sides) in [
        ("290d487200f72d9b.step.gz", 3),
        ("60eecfacf8a3d077.step.gz", 4),
        ("ef2eb61dd104b2b8.step.gz", 6),
        ("bf051b2a78149a64.step.gz", 3),
        ("b61ae416277075c9.step.gz", 4),
        ("cb3f470b2d2fa0ec.step.gz", 6),
    ] {
        let (found, defining, context, _) = one_pocket(file);
        assert_eq!((found, defining.len()), (sides, sides), "{file}");
        assert_eq!(context.len(), 2, "{file}: consulted {context:?}");
        assert!(context.iter().all(|f| !defining.contains(f)), "{file}");
    }
}
