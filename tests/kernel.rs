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
    // On edges: a block corner, a block edge and the bore's rim.
    assert_eq!(c.classify([15.0, 15.0, 10.0]), State::On);
    assert_eq!(c.classify([15.0, 15.0, 3.0]), State::On);
    assert_eq!(c.classify([0.0, 5.0, 10.0]), State::On);
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

#[test]
fn areas_and_volumes_are_exact() {
    use std::f64::consts::PI;
    let close = |got: Option<f64>, want: f64, what: &str| {
        let got = got.unwrap_or_else(|| panic!("{what}: none"));
        assert!(
            (got - want).abs() <= 1e-12 * want.abs(),
            "{what}: {got} vs {want} ({:e})",
            (got - want) / want
        );
    };
    // Box(30, 30, 20) - Cylinder(5, 20).
    let part = fixture("rejected_bored_box.step");
    close(
        part.solid_mass(0).map(|m| m.0),
        30.0 * 30.0 * 20.0 - PI * 25.0 * 20.0,
        "bored box volume",
    );
    close(
        part.solid_mass(0).map(|m| m.1),
        2.0 * (900.0 - 25.0 * PI) + 4.0 * 600.0 + 2.0 * PI * 5.0 * 20.0,
        "bored box area",
    );
    // Faces that test the boundary walk: a hemisphere bounded by a seam running up to its pole
    // and back (r = 8), a cap closed through a pole (r = 3: 2πr² of the cap), and a cylinder
    // stored as a closed B-spline surface (r = 4, h = 10) with its disc ends.
    let face = |name: &str, f: usize| fixture(name).face_mass(f).map(|m| m[0]);
    close(
        face("captured/9808e56042d8be45.step.gz", 3),
        128.0 * PI,
        "hemisphere",
    );
    close(
        face("turned_with_sphere.step", 9),
        18.0 * PI,
        "spherical cap",
    );
    let bspline = |got: Option<f64>, want: f64, what: &str| {
        let got = got.unwrap_or_else(|| panic!("{what}: none"));
        assert!((got - want).abs() <= 1e-9 * want, "{what}: {got} vs {want}");
    };
    let cylinder = "captured/14a917f282ad82d6.step.gz";
    bspline(face(cylinder, 0), 80.0 * PI, "B-spline cylinder");
    bspline(face(cylinder, 1), 16.0 * PI, "B-spline disc");
    // Torus(10, 2).
    let part = fixture("full_torus.step");
    close(
        part.solid_mass(0).map(|m| m.0),
        2.0 * PI * PI * 10.0 * 4.0,
        "torus volume",
    );
    close(
        part.solid_mass(0).map(|m| m.1),
        4.0 * PI * PI * 10.0 * 2.0,
        "torus area",
    );
    // Torus elbows with no seam edge, so the face is a band between two loops running round
    // v: Torus(20, 1) swept 200° (more than half a turn, so the nearer turn is the wrong side),
    // and Torus(7, 3) whose loops describe the 240° band rather than its 120° complement.
    for (name, big, small, sweep) in [
        ("torus_elbow_seamless.step", 20.0, 1.0, 200.0),
        ("torus_elbow_seamless_reversed_bound.step", 7.0, 3.0, 240.0),
    ] {
        let part = fixture(name);
        let turn = sweep / 360.0;
        close(
            part.solid_mass(0).map(|m| m.0),
            2.0 * PI * PI * big * small * small * turn,
            name,
        );
        close(
            part.solid_mass(0).map(|m| m.1),
            4.0 * PI * PI * big * small * turn + 2.0 * PI * small * small,
            name,
        );
    }
}
