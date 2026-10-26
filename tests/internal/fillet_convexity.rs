//! #89: a round's material side belongs to its face, not the recognition frame.
use super::common;
use quiddity::correspondence::{self, Recognition};
use quiddity::kernel::geom::Surface;
use quiddity::kernel::step::Placement;
use quiddity::{Part, read_step_file};
use std::collections::BTreeMap;

fn about_y(degrees: f64) -> Placement {
    let (s, c) = degrees.to_radians().sin_cos();
    [[c, 0., s, 0.], [0., 1., 0., 0.], [-s, 0., c, 0.]]
}

/// Compare physical faces across the specific fillet / generic blend substitution as well
/// as checking that overlapping claims never contradict each other about material side.
fn round_sides(reading: &Recognition) -> BTreeMap<usize, String> {
    let f = &reading.features;
    let mut sides = BTreeMap::new();
    for (family, values) in [
        (
            "fillets",
            f.fillets
                .iter()
                .map(|r| r.side.to_string())
                .collect::<Vec<_>>(),
        ),
        ("blends", f.blends.iter().map(|r| r.side.clone()).collect()),
    ] {
        for (faces, side) in f.defining[family].iter().zip(values) {
            for &face in faces {
                if let Some(previous) = sides.insert(face, side.clone()) {
                    assert_eq!(previous, side, "contradictory round at face {face}");
                }
            }
        }
    }
    sides
}

fn check_cylinder_sides(part: &Part, reading: &Recognition) {
    for (face, side) in round_sides(reading) {
        if matches!(part.faces[face].surface, Surface::Cylinder { .. }) {
            assert_eq!(
                part.frame_points_outward(face),
                Some(side == "convex"),
                "face {face}: {side}"
            );
        }
    }
}

/// #91: cgb241's outer R23 bend (face 47) is a convex blend in its own (file-axis) frame, as
/// it is in caller space: the normals of its two support walls (46, 48) are 92.06 degrees
/// apart, so the edge it rounds is 87.94 degrees, not the right angle of a fillet. The former
/// 27.4-degree frame took 48 as one axis and 46, 2.06 degrees off the other, as aligned.
/// The inner R20 bend (27) stays concave.
#[test]
fn cgb241_bends_are_blends_with_their_native_sides() {
    use quiddity::kernel::geom;
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let part = read_step_file(&dir.join("cadgenbench_inputs/cgb241.step")).unwrap();
    assert_eq!(part.frame_points_outward(27), Some(false));
    assert_eq!(part.frame_points_outward(47), Some(true));
    let normal = |f: usize| {
        let Surface::Plane { frame } = part.faces[f].surface else {
            panic!("{f}")
        };
        frame.z
    };
    assert!((geom::dot(normal(46), normal(48)) + 0.03589128343024717).abs() < 1e-12);
    for reading in [
        correspondence::recognise(&part),
        correspondence::caller_space_recognise(&part),
    ] {
        assert!(reading.features.fillets.is_empty());
        let f = &reading.features;
        let blend = |face: usize| {
            let i = f.defining["blends"]
                .iter()
                .position(|d| *d == vec![face])
                .unwrap_or_else(|| panic!("face {face} is not a blend"));
            &f.blends[i]
        };
        assert_eq!((blend(47).radius, blend(47).side.as_str()), (23., "convex"));
        assert_eq!(
            (blend(27).radius, blend(27).side.as_str()),
            (20., "concave")
        );
    }
}

#[test]
fn cgb202_concave_r3_faces_are_not_fillets_in_file_axes_or_five_degree_copy() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let part = read_step_file(&dir.join("cadgenbench_inputs/cgb202.step.gz")).unwrap();
    for degrees in [0., 5.] {
        let moved = part.transformed(&about_y(degrees)).unwrap();
        for reading in [
            correspondence::caller_space_recognise(&moved),
            correspondence::recognise(&moved),
        ] {
            for face in [1215, 1219] {
                assert_eq!(moved.frame_points_outward(face), Some(false));
                assert!(
                    matches!(moved.faces[face].surface, Surface::Cylinder { radius, .. } if (radius - 3.).abs() < 1e-9)
                );
                assert!(
                    !reading.features.defining["fillets"].contains(&vec![face]),
                    "{degrees} degrees: face {face}"
                );
            }
            check_cylinder_sides(&moved, &reading);
        }
    }
}

#[test]
fn every_round_keeps_its_sense_on_rotated_cgb241_cgb202_and_ctc02_ap242() {
    let mut paths = vec![common::fixtures().join("nist_ctc_02_asme1_ap242-e2.stp.gz")];
    if let Some(dir) = common::corpus_dir() {
        paths.extend([
            dir.join("cadgenbench_inputs/cgb241.step"),
            dir.join("cadgenbench_inputs/cgb202.step.gz"),
        ]);
    }
    for path in paths {
        let part = read_step_file(&path).unwrap();
        let original = round_sides(&correspondence::recognise(&part));
        assert!(!original.is_empty(), "{path:?}");
        for degrees in [5., 27.4, 60.] {
            let moved = part.transformed(&about_y(degrees)).unwrap();
            let reading = correspondence::recognise(&moved);
            assert_eq!(
                round_sides(&reading),
                original,
                "{path:?}, {degrees} degrees"
            );
            check_cylinder_sides(&moved, &reading);
            // Caller-space recognition deliberately changes the query axes. Families can
            // change, but every cylindrical round it emits must still have its native side.
            check_cylinder_sides(&moved, &correspondence::caller_space_recognise(&moved));
            for (face, side) in &original {
                if matches!(part.faces[*face].surface, Surface::Cylinder { .. }) {
                    assert_eq!(
                        moved.frame_points_outward(*face),
                        part.frame_points_outward(*face),
                        "{path:?}, {degrees} degrees: face {face}, {side}"
                    );
                }
            }
        }
    }
}

/// #91 (from #89): the inward quarter cylinders #89 stopped calling fillets are not concave
/// blends either. On mfcadpp/1000 (face 22, R7.179) and 10033 (face 7, R6.126) both planes
/// beside the cylinder's arc pass through its axis, and no plane beside it is tangent: a
/// quarter-round corner cut, with no support for a ball to roll on. CTC-03's face 35 (R6.35) is tangent to both its planes, but
/// its edge with face 7 stops 0.0035 mm short of the tangent line, a 0.032-degree convex kink,
/// so the exact smooth-join proof every blend needs refuses it; its mirror, face 38, joins
/// both planes smoothly and is a concave blend.
#[test]
fn inward_quarter_cylinders_dropped_by_89_are_not_blends() {
    use quiddity::kernel::brep::Arc;
    use quiddity::kernel::geom;
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let side_planes = |part: &Part, face: usize| {
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            panic!("{face}")
        };
        let mut found: Vec<(usize, f64)> = part
            .neighbours(face)
            .into_iter()
            .filter_map(|n| {
                let Surface::Plane { frame: plane } = part.faces[n].surface else {
                    return None;
                };
                (geom::dot(plane.z, frame.z).abs() < 1e-9).then(|| {
                    let distance = geom::dot(geom::sub(frame.origin, plane.origin), plane.z);
                    (n, distance.abs() / radius)
                })
            })
            .collect();
        found.sort_unstable_by_key(|p| p.0);
        found
    };
    let rounds = |reading: &Recognition, face: usize| {
        ["fillets", "blends"]
            .iter()
            .any(|family| reading.features.defining[*family].contains(&vec![face]))
    };
    for (file, face, through) in [
        ("mfcadpp/1000.step", 22, [1, 24]),
        ("mfcadpp/10033.step", 7, [0, 4]),
    ] {
        let part = read_step_file(&dir.join(file)).unwrap();
        assert_eq!(part.frame_points_outward(face), Some(false), "{file}");
        let found = side_planes(&part, face);
        let on_axis: Vec<usize> = found.iter().filter(|p| p.1 < 1e-9).map(|p| p.0).collect();
        assert_eq!(on_axis, through, "{file}: {found:?}");
        assert!(
            found.iter().all(|p| (p.1 - 1.).abs() > 0.1),
            "{file}: tangent {found:?}"
        );
        let reading = correspondence::recognise(&part);
        assert!(!rounds(&reading, face), "{file}: face {face}");
    }
    let part = read_step_file(&dir.join("nist/nist_ctc_03_asme1_rc.stp")).unwrap();
    for face in [35, 38] {
        assert_eq!(part.frame_points_outward(face), Some(false));
        assert!(
            side_planes(&part, face)
                .iter()
                .all(|p| (p.1 - 1.).abs() < 1e-9)
        );
    }
    assert_eq!(part.arc(35, 34), Some(Arc::Smooth));
    assert_eq!(part.arc(35, 7), Some(Arc::Convex));
    assert_eq!(part.arc(38, 7), Some(Arc::Smooth));
    assert_eq!(part.arc(38, 14), Some(Arc::Smooth));
    let reading = correspondence::recognise(&part);
    assert!(!rounds(&reading, 35));
    let f = &reading.features;
    let i = f.defining["blends"]
        .iter()
        .position(|d| *d == vec![38])
        .unwrap();
    assert_eq!(
        (f.blends[i].radius, f.blends[i].side.as_str()),
        (6.35, "concave")
    );
}
