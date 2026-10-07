//! Kernel behaviour on real parts read from STEP.

use std::path::Path;

use quiddity::kernel::classify::{Classifier, State};
use quiddity::read_step_file;

fn fixture(name: &str) -> quiddity::Part {
    read_step_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

#[test]
fn classifier_sees_material_hole_and_outside() {
    // Box(30, 30, 20) - Cylinder(5, 20): a 30 × 30 × 20 block with a Ø10 through hole.
    let part = fixture("rejected_bored_box.step");
    let c = Classifier::new(&part);
    assert_eq!(c.classify([10.0, 10.0, 0.0]), State::In);
    assert_eq!(c.classify([0.0, 0.0, 0.0]), State::Out, "inside the hole");
    assert_eq!(c.classify([0.0, 0.0, 15.0]), State::Out, "above the block");
    assert_eq!(c.classify([20.0, 0.0, 0.0]), State::Out, "beside the block");
    // Just inside each kind of boundary.
    assert_eq!(c.classify([14.999, 0.0, 0.0]), State::In);
    assert_eq!(c.classify([5.001, 0.0, 0.0]), State::In);
    assert_eq!(c.classify([4.999, 0.0, 0.0]), State::Out);
}

#[test]
fn reading_is_deterministic_and_topology_is_closed() {
    let a = fixture("turned_with_sphere.step");
    let b = fixture("turned_with_sphere.step");
    assert_eq!(a.faces.len(), b.faces.len());
    for s in 0..a.solids.len() {
        assert!(a.solid_is_valid(s));
    }
    for f in 0..a.faces.len() {
        assert_eq!(a.uv_bounds(f), b.uv_bounds(f));
        assert_eq!(a.neighbours(f), b.neighbours(f));
    }
}
