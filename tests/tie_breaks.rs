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

