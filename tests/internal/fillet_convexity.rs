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

#[test]
fn cgb241_inner_bend_is_concave_and_outer_bend_returns_to_a_blend() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let part = read_step_file(&dir.join("cadgenbench_inputs/cgb241.step")).unwrap();
    assert_eq!(part.frame_points_outward(27), Some(false));
    assert_eq!(part.frame_points_outward(47), Some(true));
    let reading = correspondence::recognise(&part);
    assert!(!reading.features.defining["fillets"].contains(&vec![27]));
    assert!(!reading.features.defining["fillets"].contains(&vec![47]));
    assert!(reading.features.defining["blends"].contains(&vec![47]));
    assert_eq!(reading.features.blends.len(), 6);
    let sides = round_sides(&reading);
    assert_eq!(sides[&27], "concave");
    assert_eq!(sides[&47], "convex");
    let caller = correspondence::caller_space_recognise(&part);
    assert_eq!(round_sides(&caller)[&27], "concave");
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
            if path.file_name().unwrap() == "nist_ctc_02_asme1_ap242-e2.stp.gz" && degrees == 5. {
                // #91: this exact Y5 pose gains a file-axis frame on minority
                // directions. Pin its different query, not an equivariance exception
                // on generic poses. The five principal-run labels are frame-filtered.
                let quiddity::frames::RecordFrame::Inferred(frame) = reading.frame else {
                    panic!("frame")
                };
                assert!(
                    [frame.x, frame.y, frame.z]
                        .iter()
                        .all(|a| a.iter().any(|c| c.abs() == 1.))
                );
                let mut expected = original.clone();
                for face in [399, 519, 548, 625, 631] {
                    assert_eq!(expected.remove(&face).as_deref(), Some("convex"));
                }
                assert_eq!(round_sides(&reading), expected);
                // Preserve the strict full-face check under a generic rotation with
                // no file-axis frame, including all of the displaced round labels.
                let pose = super::frames::vote_only_pose(&about_y(degrees));
                let generic = part.transformed(&pose).unwrap();
                super::frames::assert_no_file_axis_frame(&generic);
                let generic_reading = correspondence::recognise(&generic);
                assert_eq!(round_sides(&generic_reading), original);
                check_cylinder_sides(&generic, &generic_reading);
            } else {
                assert_eq!(
                    round_sides(&reading),
                    original,
                    "{path:?}, {degrees} degrees"
                );
            }
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
