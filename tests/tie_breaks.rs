//! ADR 0011: topology numbering and support parametrisation cannot establish machining roles.
mod common;
use quiddity::features::{self, Context};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};

fn motions() -> Vec<Placement> {
    let mut out = vec![
        IDENTITY,
        [
            [1., 0., 0., 123.456],
            [0., 1., 0., -78.9],
            [0., 0., 1., 41.3],
        ],
    ];
    for (axis, degrees) in [(2, 30_f64), (0, 30.), (0, 90.), (1, 180.)] {
        let (s, c) = degrees.to_radians().sin_cos();
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut p = IDENTITY;
        p[a][a] = c;
        p[a][b] = -s;
        p[b][a] = s;
        p[b][b] = c;
        out.push(p);
    }
    out
}

#[test]
fn symmetric_double_mouth_refuses_under_motion() {
    let path = common::fixtures().join("ties/symmetric_double_mouth.step");
    for at in motions() {
        let part = read_step_file_placed(&path, &at).unwrap();
        let found = features::holes::discover(&Context::new(&part), &[]);
        assert!(found.is_empty(), "{at:?}: {found:?}");
    }
}

#[test]
fn equal_mouths_choose_by_inward_geometry_under_motion() {
    let path = common::fixtures().join("ties/asymmetric_double_mouth.step");
    let mut faces = None;
    for at in motions() {
        let part = read_step_file_placed(&path, &at).unwrap();
        let found = features::holes::discover(&Context::new(&part), &[]);
        assert_eq!(found.len(), 1, "{at:?}");
        let hole = &found[0].record;
        assert_eq!(hole.cbore.as_ref().unwrap().depth, 8.0);
        assert_eq!(hole.cbore.as_ref().unwrap().diameter, 12.0);
        assert_eq!(hole.diameter, 6.0);
        let expected = [
            at[0][3] + 15. * at[0][2],
            at[1][3] + 15. * at[1][2],
            at[2][3] + 15. * at[2][2],
        ];
        assert!(
            quiddity::kernel::py::dist(&hole.location, &expected) < 1e-6,
            "{hole:?}"
        );
        let mut got = found[0].defining.clone();
        got.sort_unstable();
        assert_eq!(faces.get_or_insert_with(|| got.clone()), &got);
    }
}

#[test]
fn symmetric_square_pocket_refuses_a_semantic_run_axis() {
    let path = common::fixtures().join("ties/square_pocket.step");
    // Principal-axis motions preserve this family's explicit axis-letter contract.
    for at in motions().into_iter().filter(|p| {
        p.iter()
            .flatten()
            .all(|v| v.abs() < 1e-12 || (v.abs() - 1.).abs() < 1e-12)
            || *p == motions()[1]
    }) {
        let part = read_step_file_placed(&path, &at).unwrap();
        let found = features::pockets::discover(&Context::new(&part));
        assert!(found.is_empty(), "{at:?}: {found:?}");
    }
}

#[test]
fn crossing_equal_patterns_refuse_under_motion_and_member_permutation() {
    use features::holes::{Bottom, HoleRecord};
    for at in motions() {
        let mut holes: Vec<_> = [
            [0., 0., 0.],
            [10., 0., 0.],
            [20., 0., 0.],
            [0., 10., 0.],
            [0., 20., 0.],
        ]
        .map(|point| HoleRecord {
            axis: [at[0][2], at[1][2], at[2][2]],
            location: [0, 1, 2]
                .map(|i| at[i][3] + (0..3).map(|j| at[i][j] * point[j]).sum::<f64>()),
            diameter: 2.,
            depth: 5.,
            bottom: Bottom::Through,
            cbore: None,
            spotface: None,
            csink: None,
        })
        .into();
        for _ in 0..holes.len() {
            assert!(
                features::hole_patterns::recognise_hole_patterns(&holes).is_empty(),
                "{at:?}"
            );
            holes.rotate_left(1);
        }
        holes.reverse();
        assert!(features::hole_patterns::recognise_hole_patterns(&holes).is_empty());
    }
}

#[test]
fn equidistant_support_planes_refuse_under_principal_motion() {
    let path = common::fixtures().join("ties/square_pocket.step");
    for at in [motions()[0], motions()[1], motions()[4], motions()[5]] {
        let part = read_step_file_placed(&path, &at).unwrap();
        // The intact bottom is bounded by four equally distant side planes.
        let face = (0..part.faces.len())
            .find(|&f| (part.face_mass(f).unwrap()[0] - 1600.).abs() < 1e-6)
            .unwrap();
        let (axis, _) = features::planes::axis_aligned_axis(&part, face).unwrap();
        let centre = part.face_bounds(face).centre();
        for legacy_flag in [false, true] {
            let got = features::planes::nearest_axis_aligned_planes(
                &part,
                face,
                centre,
                axis,
                legacy_flag,
            );
            assert!(got.is_empty(), "{at:?}: {got:?}");
        }
    }
}

#[test]
fn equally_supported_turning_axes_refuse_under_principal_motion() {
    let path = common::fixtures().join("ties/equal_axis_shafts.step");
    for at in [motions()[0], motions()[1], motions()[4], motions()[5]] {
        let part = read_step_file_placed(&path, &at).unwrap();
        let ctx = Context::new(&part);
        let supported = features::cylinders::full_cylinders(ctx.cylinders());
        let mut counts = std::collections::BTreeMap::new();
        for c in supported {
            *counts.entry(c.axis).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 2, "{counts:?}");
        assert_eq!(
            counts.values().next(),
            counts.values().next_back(),
            "{counts:?}"
        );
        assert!(features::turned_steps::discover(&ctx).is_empty(), "{at:?}");
    }
}

#[test]
fn equal_pitch_grid_uses_population_to_assign_rows_under_motion() {
    use features::hole_patterns::HolePattern;
    use features::holes::{Bottom, HoleRecord};
    for at in motions() {
        let mut holes: Vec<_> = (0..3)
            .flat_map(|x| (0..4).map(move |y| [10. * x as f64, 10. * y as f64, 0.]))
            .map(|p| HoleRecord {
                axis: [at[0][2], at[1][2], at[2][2]],
                location: [0, 1, 2]
                    .map(|i| at[i][3] + (0..3).map(|j| at[i][j] * p[j]).sum::<f64>()),
                diameter: 2.,
                depth: 5.,
                bottom: Bottom::Through,
                cbore: None,
                spotface: None,
                csink: None,
            })
            .collect();
        for _ in 0..holes.len() {
            let patterns = features::hole_patterns::recognise_hole_patterns(&holes);
            let [
                HolePattern::RectGrid {
                    rows,
                    cols,
                    row_pitch,
                    col_pitch,
                    ..
                },
            ] = patterns.as_slice()
            else {
                panic!("{at:?}: {patterns:?}")
            };
            assert_eq!((*rows, *cols, *row_pitch, *col_pitch), (3, 4, 10., 10.));
            holes.rotate_left(1);
        }
    }
}

#[test]
fn contradictory_equal_distance_end_supports_report_unknown_under_motion() {
    use features::cylinders::Segment;
    use features::stacks::{End, classify_end};
    use quiddity::kernel::brep::{Edge, Face, Loop, Part};
    use quiddity::kernel::geom::{Curve, Frame, Surface};
    for at in motions() {
        let point = |p: [f64; 3]| {
            [0, 1, 2].map(|i| at[i][3] + (0..3).map(|j| at[i][j] * p[j]).sum::<f64>())
        };
        let dir = |p: [f64; 3]| [0, 1, 2].map(|i| (0..3).map(|j| at[i][j] * p[j]).sum::<f64>());
        let frame = Frame {
            origin: point([0.; 3]),
            x: dir([1., 0., 0.]),
            y: dir([0., 1., 0.]),
            z: dir([0., 0., 1.]),
        };
        let edge = Edge {
            curve: Curve::Circle { frame, radius: 2. },
            start: point([2., 0., 0.]),
            end: point([2., 0., 0.]),
            vertices: (0, 0),
            same_sense: true,
            samples: vec![point([2., 0., 0.]), point([-2., 0., 0.])],
        };
        for reverse in [false, true] {
            let face = |surface, reversed| Face {
                surface,
                reversed,
                loops: vec![Loop {
                    edges: vec![(0, true)],
                    vertex: None,
                }],
                solid: None,
                pcurves: vec![],
            };
            let part = Part::new(
                vec![
                    face(Surface::Cylinder { frame, radius: 2. }, false),
                    face(Surface::Plane { frame }, reverse),
                    face(Surface::Plane { frame }, !reverse),
                ],
                vec![edge.clone()],
                vec![],
            );
            let s = quiddity::kernel::geom::dot(frame.origin, frame.z);
            let segment = Segment {
                solid: 0,
                diameter: 4.,
                axis: 2,
                axis_point: frame.origin,
                direction: frame.z,
                s_lo: s - 1.,
                s_hi: s,
                external: false,
                faces: vec![0],
            };
            assert_eq!(
                classify_end(&part, &segment, s, true),
                (End::Unknown, vec![])
            );
        }
    }
}

#[test]
fn tied_countersink_composition_refuses_under_motion_and_seat_permutation() {
    use features::countersinks::CounterSink;
    use features::evidence::Occurrence;
    let path = common::fixtures().join("golden_simple_through_hole.step");
    for at in motions() {
        let part = read_step_file_placed(&path, &at).unwrap();
        let ctx = Context::new(&part);
        let holes = features::holes::discover(&ctx, &[]);
        let hole = &holes[0].record;
        let cs = CounterSink {
            axis: hole.axis,
            location: [0, 1, 2].map(|i| hole.location[i] - hole.axis[i]),
            major_diameter: hole.diameter + 2.,
            drill_diameter: hole.diameter,
            included_angle: 90.,
            depth: 1.,
        };
        let mut seats = vec![
            Occurrence {
                record: cs.clone(),
                defining: vec![1],
                context: vec![],
            },
            Occurrence {
                record: cs,
                defining: vec![7],
                context: vec![],
            },
        ];
        assert!(features::holes::discover(&ctx, &seats).is_empty());
        seats.reverse();
        assert!(features::holes::discover(&ctx, &seats).is_empty());
    }
}

#[test]
fn a_symmetric_internal_relief_keeps_its_undirected_through_bore_under_motion() {
    let path = common::fixtures().join("captured/ece751715f714026.step.gz");
    let mut faces = None;
    for at in motions() {
        let part = read_step_file_placed(&path, &at).unwrap();
        let found = features::holes::discover(&Context::new(&part), &[]);
        assert_eq!(found.len(), 1, "{at:?}");
        assert_eq!(found[0].record.depth, 20.);
        assert_eq!(found[0].record.diameter, 10.);
        assert!(found[0].record.cbore.is_none() && found[0].record.spotface.is_none());
        let mut got = found[0].defining.clone();
        got.sort_unstable();
        assert_eq!(faces.get_or_insert_with(|| got.clone()), &got);
    }
}

#[test]
fn bevel_uses_its_cutting_plane_to_resolve_equal_support_distances_under_motion() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let path = dir.join("mfcadpp_holdout/483.step");
    let part = read_step_file_placed(&path, &IDENTITY).unwrap();
    let found = features::angled_steps::discover(&Context::new(&part));
    assert_eq!(found.len(), 3);
    let real = found.iter().find(|o| o.defining == [13]).unwrap();
    // Independent OpenCascade plane equation: x=6.095095 at y=29.381039 is on the
    // bevel itself; the removed corner is the opposite x support, 10.137419.
    assert_eq!(real.record.corner, Some([10.137, 29.381, 13.064]));
    let read = |at: &Placement| {
        let framed = quiddity::frames::prepare_framed_file(&path, at).unwrap();
        let mut found: Vec<_> = features::angled_steps::discover(&Context::new(&framed.part))
            .into_iter()
            .map(|o| o.defining)
            .collect();
        found.sort();
        found
    };
    let baseline = read(&IDENTITY);
    for at in motions() {
        assert_eq!(read(&at), baseline, "{at:?}");
    }
}

#[test]
fn a_square_recess_needs_a_covariant_body_relative_run_for_correspondence() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let path = dir.join("mfcadpp/10049.step");
    let read = |at: &Placement| {
        let part = read_step_file_placed(&path, at).unwrap();
        let found = features::recognise(&part);
        quiddity::correspondence::fingerprint(&part, &found)
            .features
            .into_iter()
            .filter(|f| f.family == "pockets")
            .map(|f| (f.faces, (f.axis.unwrap(), f.sizes)))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let baseline = read(&IDENTITY);
    assert_eq!(baseline.len(), 3);
    for at in [
        motions()[1],
        motions()[4],
        motions()[5],
        [
            [0., 0., 1., 123.456],
            [1., 0., 0., -78.9],
            [0., 1., 0., 41.3],
        ],
    ] {
        let moved = read(&at);
        assert_eq!(
            baseline.keys().collect::<Vec<_>>(),
            moved.keys().collect::<Vec<_>>()
        );
        for (faces, (axis, sizes)) in &baseline {
            let expected = [0, 1, 2].map(|i| (0..3).map(|j| at[i][j] * axis[j]).sum::<f64>());
            let (actual, got_sizes) = &moved[faces];
            assert!(
                (quiddity::kernel::py::dot(&expected, actual).abs() - 1.).abs() < 1e-9,
                "{at:?} {faces:?}"
            );
            assert_eq!(sizes, got_sizes);
        }
    }
}

#[test]
fn a_cube_refuses_competing_circular_pattern_axes_under_motion() {
    let path = common::fixtures().join("ties/cube.step");
    for at in motions() {
        let part = read_step_file_placed(&path, &at).unwrap();
        assert!(part.solid_is_valid(0));
        // Every one of the three fourfold axes has equal analytic face-area support.
        // The four side faces cannot silently become a different claimed pattern.
        let found = features::circular_face_patterns::discover(&Context::new(&part));
        assert!(found.is_empty(), "{at:?}: {found:?}");
    }
}

#[test]
fn a_symmetric_cylinder_refuses_an_arbitrary_boss_free_end_under_motion() {
    let path = common::fixtures().join("ties/symmetric_cylinder.step");
    for at in motions() {
        let part = read_step_file_placed(&path, &at).unwrap();
        let ctx = Context::new(&part);
        let cylinders: Vec<_> = ctx.cylinders().iter().filter(|c| c.external).collect();
        assert_eq!(cylinders.len(), 1);
        assert!(features::bosses::discover(&ctx).is_empty(), "{at:?}");
    }
}

#[test]
fn tied_boss_end_states_use_the_body_origin_after_motion() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let path = dir.join("nist/nist_ctc_05_asme1_rd.stp");
    let baseline = read_step_file_placed(&path, &IDENTITY).unwrap();
    let before = features::bosses::discover(&Context::new(&baseline));
    assert!(before.iter().any(|b| b.defining == [1, 182]));
    for at in motions() {
        let moved = read_step_file_placed(&path, &at).unwrap();
        let after = features::bosses::discover(&Context::new(&moved));
        assert_eq!(before.len(), after.len(), "{at:?}");
        for b in &before {
            let a = after.iter().find(|a| a.defining == b.defining).unwrap();
            assert_eq!(b.context, a.context, "{at:?}: {:?}", b.defining);
            for (i, row) in at.iter().enumerate() {
                let point = row[3] + (0..3).map(|j| row[j] * b.record.location[j]).sum::<f64>();
                let axis = (0..3).map(|j| row[j] * b.record.axis[j]).sum::<f64>();
                assert!((point - a.record.location[i]).abs() < 1e-6);
                assert!((axis - a.record.axis[i]).abs() < 1e-8);
            }
        }
    }
}
