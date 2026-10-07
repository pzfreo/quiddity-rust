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
fn arcs_read_convex_rims_and_concave_pocket_corners() {
    use quiddity::kernel::brep::Arc;
    // Box(30, 30, 20) - Cylinder(5, 20): every edge, the bore rims included, is a convex wedge.
    let part = fixture("rejected_bored_box.step");
    for a in 0..part.faces.len() {
        for b in (0..part.faces.len()).filter(|&b| b != a) {
            if let Some(arc) = part.arc(a, b) {
                assert_eq!(arc, Arc::Convex, "faces {a} and {b}");
            }
        }
    }
    // A blind pocket: its floor meets its walls in concave edges, read the same from either side.
    let part = fixture("rejected_internal_pocket.step");
    let mut concave = 0;
    for a in 0..part.faces.len() {
        for b in (0..part.faces.len()).filter(|&b| b != a) {
            if let Some(arc) = part.arc(a, b) {
                assert_eq!(part.arc(b, a), Some(arc), "symmetric for faces {a} and {b}");
                concave += usize::from(arc == Arc::Concave);
            }
        }
    }
    assert!(
        concave >= 8,
        "the pocket floor and its four walls, both ways round"
    );
}

#[test]
fn arcs_at_closed_edges_ignore_their_recorded_direction() {
    use quiddity::kernel::brep::Arc;
    // The bore rims are full circles; flip every face's use of them and the read must not change.
    let mut part = fixture("rejected_bored_box.step");
    let closed: Vec<bool> = part.edges.iter().map(|e| e.is_closed()).collect();
    for face in &mut part.faces {
        for lp in &mut face.loops {
            for (e, forward) in &mut lp.edges {
                if closed[*e] {
                    *forward = !*forward;
                }
            }
        }
    }
    let mut rims = 0;
    for a in 0..part.faces.len() {
        for b in (0..part.faces.len()).filter(|&b| b != a) {
            if let Some(arc) = part.arc(a, b) {
                assert_eq!(arc, Arc::Convex, "faces {a} and {b}");
                rims += usize::from(part.shared_edges(a, b).iter().any(|&e| closed[e]));
            }
        }
    }
    assert!(rims >= 4, "both rims, both ways round");
}
