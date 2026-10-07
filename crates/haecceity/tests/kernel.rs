//! Kernel behaviour on real parts read from STEP.

use std::path::Path;

use haecceity::classify::{Classifier, State};
use haecceity::read_step_file;

fn fixture(name: &str) -> haecceity::Part {
    read_step_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
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
    use haecceity::brep::Arc;
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
    use haecceity::brep::Arc;
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
    // A hemisphere (r = 12.7) bounded by one closed great circle through both poles, starting
    // 37° round from one, so a quadrature panel would straddle each pole.
    let part = fixture("hemisphere_one_edge.step");
    let r: f64 = 12.7;
    close(
        part.solid_mass(0).map(|m| m.0),
        2.0 / 3.0 * PI * r.powi(3),
        "hemisphere volume",
    );
    close(
        part.solid_mass(0).map(|m| m.1),
        3.0 * PI * r * r,
        "hemisphere area",
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

#[test]
fn swept_faces_are_closed_prisms_of_the_face() {
    use haecceity::geom::{Surface, scale};
    use haecceity::sweep::extrude_face;
    for name in [
        "golden_circular_blind_step.step",
        "golden_oblique_through_step.step",
        "edge_open_pocket_floor_hole.step",
    ] {
        let part = fixture(name);
        let mut swept = 0;
        for face in 0..part.faces.len() {
            let Surface::Plane { frame } = part.faces[face].surface else {
                continue;
            };
            let Some([area, _]) = part.face_mass(face) else {
                continue;
            };
            // Along the outward normal and against it, from a moved start.
            for sign in [2.5, -2.5] {
                let Some(prism) = extrude_face(&part, face, [0.5, -1.0, 2.0], scale(frame.z, sign))
                else {
                    continue;
                };
                swept += 1;
                assert!(prism.solid_is_valid(0), "{name} face {face}");
                let (volume, _) = prism.solid_mass(0).unwrap();
                assert!(
                    (volume - area * 2.5).abs() <= 1e-9 * volume,
                    "{name} face {face}: {volume} vs {}",
                    area * 2.5
                );
            }
        }
        assert!(swept > 0, "{name}: no face swept");
        if name == "edge_open_pocket_floor_hole.step" {
            // The pocket floor and the plate's bottom, each holed by a closed circle.
            let holed = (0..part.faces.len())
                .filter(|&f| {
                    part.faces[f].loops.len() > 1
                        && part
                            .face_edges(f)
                            .iter()
                            .any(|&e| part.edges[e].is_closed())
                        && extrude_face(&part, f, [0.0; 3], [0.0, 0.0, 1.0]).is_some()
                })
                .count();
            assert!(holed >= 2, "{name}: {holed} holed faces swept");
        }
    }
}

#[test]
fn prism_probes_measure_swept_regions() {
    use haecceity::geom::Bounds;
    use haecceity::volume::{Prism, PrismEdge, Probe, common_volume, probe_volume};
    // Box(30, 10, 40) at y = -5 less a U-section slot (10 wide, radius 3, flat 4) cut 20 deep
    // from z = 20 down to the cap at z = 0: open at y = 0.
    let part = fixture("captured/4e090fe361c883ac.step.gz");
    let solid = Classifier::new(&part);
    let straight = |pts: &[[f64; 2]]| PrismEdge {
        points: pts.iter().map(|p| [p[0], p[1], 0.0]).collect(),
        straight: true,
    };
    let rectangle = |x0: f64, x1: f64, y0: f64, y1: f64| {
        vec![
            straight(&[[x0, y0], [x1, y0]]),
            straight(&[[x1, y0], [x1, y1]]),
            straight(&[[x1, y1], [x0, y1]]),
            straight(&[[x0, y1], [x0, y0]]),
        ]
    };
    // A rectangle swept along z is the box with the same corners, wherever it lies.
    for (x0, x1, y0, y1, z0, z1) in [
        (-10.0, -6.0, -8.0, -2.0, -5.0, 5.0),
        (-8.0, 8.0, -6.0, 2.0, 5.0, 15.0),
        (-3.0, 3.0, -2.0, 1.0, -4.0, 12.0),
    ] {
        let prism = Probe::Prism(Prism {
            axis: 2,
            lo: z0,
            hi: z1,
            loops: vec![rectangle(x0, x1, y0, y1)],
        });
        let cube = Probe::Box(Bounds {
            min: [x0, y0, z0],
            max: [x1, y1, z1],
        });
        let (a, b) = (common_volume(&solid, &prism), common_volume(&solid, &cube));
        assert!((a - b).abs() <= 1e-9 * b.max(1.0), "{a} vs {b}");
        assert!((probe_volume(&prism) - probe_volume(&cube)).abs() <= 1e-9);
    }
    // The slot's own section swept along the slot is empty; swept on beyond the cap it is full.
    let arc = |cx: f64, from: f64, to: f64| PrismEdge {
        points: (0..=64)
            .map(|i| {
                let t = from + (to - from) * i as f64 / 64.0;
                [cx + 3.0 * t.cos(), 3.0 * t.sin(), 0.0]
            })
            .collect(),
        straight: false,
    };
    let half_pi = std::f64::consts::FRAC_PI_2;
    let section = || {
        vec![
            straight(&[[-5.0, 0.0], [5.0, 0.0]]),
            arc(2.0, 0.0, -half_pi),
            straight(&[[2.0, -3.0], [-2.0, -3.0]]),
            arc(-2.0, -half_pi, -2.0 * half_pi),
        ]
    };
    let swept = |lo: f64, hi: f64| {
        Probe::Prism(Prism {
            axis: 2,
            lo,
            hi,
            loops: vec![section()],
        })
    };
    assert_eq!(common_volume(&solid, &swept(0.0, 20.0)), 0.0);
    let beyond = swept(-10.0, 0.0);
    let full = probe_volume(&beyond);
    // Chords inside the arcs: 64 per quarter lose about 0.02 mm³ over the 10 mm.
    assert!(
        (full - 10.0 * (4.0 * 3.0 + 9.0 * half_pi)).abs() < 0.05,
        "{full}"
    );
    assert!((common_volume(&solid, &beyond) - full).abs() <= 1e-9 * full);
}
