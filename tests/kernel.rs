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
    // On a face, as BRepClass3d reports within its tolerance.
    assert_eq!(c.classify([15.0, 3.0, 2.0]), State::On);
    assert_eq!(c.classify([5.0, 0.0, 0.0]), State::On);
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

#[test]
fn arcs_read_convex_box_edges_and_concave_bore_rims() {
    use quiddity::kernel::brep::Arc;
    use quiddity::kernel::geom::Surface;
    // Box(30, 30, 20) - Cylinder(5, 20): plane-plane box edges are convex; the bore wall meets
    // the top and bottom faces in concave rims.
    let part = fixture("rejected_bored_box.step");
    let planes: Vec<usize> = (0..part.faces.len())
        .filter(|&f| matches!(part.faces[f].surface, Surface::Plane { .. }))
        .collect();
    let bore = (0..part.faces.len())
        .find(|&f| matches!(part.faces[f].surface, Surface::Cylinder { .. }))
        .unwrap();
    for &a in &planes {
        for &b in &planes {
            if let Some(arc) = part.arc(a, b) {
                assert_eq!(arc, Arc::Convex, "box faces {a} and {b}");
            }
        }
        if let Some(arc) = part.arc(a, bore) {
            assert_eq!(arc, Arc::Concave);
            assert_eq!(part.arc(bore, a), Some(Arc::Concave), "symmetric");
        }
    }
}
