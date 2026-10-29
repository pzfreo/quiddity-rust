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

/// #120 supersedes #91's right-angle boundary: the R23 face 47 rounds an 87.94°
/// convex edge between tangent walls and is now a fillet. For #127, the inner R20
/// face 27 passes the same proof as a concave fillet. Both frames agree on the sides.
#[test]
fn cgb241_bends_keep_their_native_sides_and_convex_round_is_a_fillet() {
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
        let f = &reading.features;
        let fillet = f.defining["fillets"]
            .iter()
            .position(|d| *d == vec![47])
            .unwrap();
        assert_eq!(
            (f.fillets[fillet].radius, f.fillets[fillet].side),
            (23., "convex")
        );
        assert!(f.defining["blends"].iter().all(|d| !d.contains(&47)));
        let inner = f.defining["fillets"]
            .iter()
            .position(|d| *d == vec![27])
            .unwrap();
        assert_eq!(
            (f.fillets[inner].radius, f.fillets[inner].side),
            (20., "concave")
        );
        assert!(!f.defining["blends"].iter().any(|d| d.contains(&27)));
    }
}

/// #120: these outer bend skins stop at a cut, not a second tangent wall. The old
/// axis-aligned shortcut accepted them without checking tangency or the swept angle.
#[test]
fn truncated_sm_hanger_bend_skins_keep_sheet_ownership_without_false_fillets() {
    use quiddity::kernel::geom;
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let part = read_step_file(&dir.join("ttt_inputs/sm-hanger.step")).unwrap();
    for face in [73, 76] {
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            panic!("face {face}")
        };
        let Surface::Plane { frame: wall } = part.faces[64].surface else {
            panic!("wall 64")
        };
        assert_eq!(radius, 7.);
        assert!(part.neighbours(face).contains(&64));
        let error = (geom::dot(geom::sub(frame.origin, wall.origin), wall.z).abs() - radius).abs();
        assert!((error - 4.).abs() < 1e-9, "face {face}: {error}");
    }
    for degrees in [0., 5.] {
        let moved = part.transformed(&about_y(degrees)).unwrap();
        for reading in [
            correspondence::recognise(&moved),
            correspondence::caller_space_recognise(&moved),
        ] {
            let f = &reading.features;
            let sheet = &f.sheet_metal_bodies[0];
            for (bend, outer, inner) in [(6, 73, 75), (7, 76, 85)] {
                let bend = &sheet.bends[bend];
                assert_eq!(bend.inner_radius, 3.);
                assert!((bend.angle_degrees - 90.).abs() < 1e-9);
                assert!(
                    bend.face_pairs
                        .iter()
                        .any(|p| { p.first_face == outer && p.second_face == inner })
                );
                assert!(f.defining["sheet_metal_bodies"][0].contains(&outer));
                assert!(f.defining["fillets"].iter().all(|d| !d.contains(&outer)));
            }
        }
    }
}

/// #120: a proved round does not replace a more specific accepted defining claim.
/// Its generic blend must remain when the fillet proposal is suppressed.
#[test]
fn specific_defining_claims_keep_gusset_and_pattern_rounds_out_of_fillets() {
    for (file, faces, family) in [
        (
            "b9b085119521ca83.step.gz",
            &[2, 6, 12, 13][..],
            "gusset_ribs",
        ),
        ("76434c8233b191c6.step.gz", &[2, 6][..], "gusset_ribs"),
        ("9d0b60cef180d03e.step.gz", &[2, 6][..], "gusset_ribs"),
        ("a96691391681646d.step.gz", &[2][..], "gusset_ribs"),
        (
            "outer_profiles/parts/9bf8ce472067c7ac.step.gz",
            &[2, 3, 7][..],
            "circular_face_patterns",
        ),
    ] {
        let part = read_step_file(&common::fixtures().join("captured").join(file)).unwrap();
        let reading = correspondence::recognise(&part);
        let defining = &reading.features.defining;
        for face in faces {
            assert!(
                defining[family].iter().any(|d| d.contains(face)),
                "{file}: {face}"
            );
            assert!(
                defining["blends"].iter().any(|d| d.contains(face)),
                "{file}: {face}"
            );
            assert!(
                defining["fillets"].iter().all(|d| !d.contains(face)),
                "{file}: {face}"
            );
        }
    }
}

// #89: retain the exact face/radius/non-fillet guard in the shared motion readings.
fn cgb202_guard(part: &Part, reading: &Recognition) {
    for face in [1215, 1219] {
        assert_eq!(part.frame_points_outward(face), Some(false));
        assert!(
            matches!(part.faces[face].surface, Surface::Cylinder { radius, .. } if (radius - 3.).abs() < 1e-9)
        );
        assert!(
            !reading.features.defining["fillets"].contains(&vec![face]),
            "face {face}"
        );
    }
    check_cylinder_sides(part, reading);
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
        let reading = correspondence::recognise(&part);
        let cgb202 = path.file_name().unwrap() == "cgb202.step.gz";
        if cgb202 {
            cgb202_guard(&part, &reading);
            cgb202_guard(&part, &correspondence::caller_space_recognise(&part));
        }
        let original = round_sides(&reading);
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
            let caller = correspondence::caller_space_recognise(&moved);
            check_cylinder_sides(&moved, &caller);
            if cgb202 && degrees == 5. {
                cgb202_guard(&moved, &reading);
                cgb202_guard(&moved, &caller);
            }
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
/// so the exact smooth-join proof every blend needs refuses it. For #127, both it and
/// its smooth mirror 38 pass the fillet proof: tangent planes at model precision and a
/// sweep within #120's unchanged 0.5° band. The through-axis cuts remain excluded.
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
    let f = &reading.features;
    for face in [35, 38] {
        let i = f.defining["fillets"]
            .iter()
            .position(|d| *d == vec![face])
            .unwrap();
        assert_eq!((f.fillets[i].radius, f.fillets[i].side), (6.35, "concave"));
        assert!(!f.defining["blends"].contains(&vec![face]));
    }
}

/// #120: drafted tangent walls prove the same R1 fillets on both mirrored flange sides.
#[test]
fn cast_bearing_rounds_are_fillets_at_any_angle_and_direction() {
    use quiddity::framed_records::Rigid;
    use quiddity::kernel::geom;
    let part = read_step_file(&common::fixtures().join("cast_bearing_unit.step.gz")).unwrap();
    let original = correspondence::recognise(&part);
    check_cast_rounds(&part, &original);
    check_cast_rounds(&part, &correspondence::caller_space_recognise(&part));
    for degrees in [5., 27.4, 60.] {
        let mut placement = about_y(degrees);
        placement[0][3] = 17.;
        placement[1][3] = -23.;
        placement[2][3] = 41.;
        let moved = part.transformed(&placement).unwrap();
        let reading = correspondence::recognise(&moved);
        check_cast_rounds(&moved, &reading);
        check_cast_rounds(&moved, &correspondence::caller_space_recognise(&moved));
        for old in original
            .fingerprints
            .features
            .iter()
            .filter(|f| f.family == "fillets")
        {
            let new = reading
                .fingerprints
                .features
                .iter()
                .find(|f| f.family == "fillets" && f.faces == old.faces)
                .unwrap();
            assert_eq!(old.sizes, new.sizes);
            assert_eq!(old.traits, new.traits);
            let expected = Rigid::from_placement(&placement).direction(old.axis.unwrap());
            assert!((geom::dot(expected, new.axis.unwrap()).abs() - 1.).abs() < 1e-12);
        }
    }
}

fn check_cast_rounds(part: &Part, reading: &Recognition) {
    use quiddity::features::bevel::transverse_wall_corner;
    use quiddity::features::fillets::fillet_anchor;
    use quiddity::kernel::geom;
    assert_eq!(reading.features.fillets.len(), 8);
    let families = reading.features.families();
    let records = &families.iter().find(|(f, _)| f == "fillets").unwrap().1;
    for face in [14, 25, 16, 27, 24, 18, 22, 20] {
        let i = reading.features.defining["fillets"]
            .iter()
            .position(|d| *d == vec![face])
            .unwrap_or_else(|| panic!("face {face}: missing fillet"));
        let record = &records[i];
        assert_eq!(record["radius"], 1.);
        assert_eq!(record["side"], "convex");
        assert_eq!(record["turned"], false);
        let axis = match record["axis"].as_str() {
            Some("x") => [1., 0., 0.],
            Some("y") => [0., 1., 0.],
            Some("z") => [0., 0., 1.],
            _ => serde_json::from_value(record["axis"].clone()).unwrap(),
        };
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            panic!("{face}")
        };
        assert!((radius - 1.).abs() < 1e-12);
        assert_eq!(part.frame_points_outward(face), Some(true));
        let at = fillet_anchor(part, face).unwrap();
        let corner = transverse_wall_corner(part, face, at, frame.z).unwrap();
        assert!(
            (geom::dot(corner.normals[0], corner.normals[1]).abs() - 4_f64.to_radians().sin())
                .abs()
                < 1e-12
        );
        for n in corner.normals {
            let distance = geom::dot(geom::sub(frame.origin, corner.point), n).abs();
            assert!((distance - radius).abs() <= 1e-7);
        }
        assert!((geom::norm(axis) - 1.).abs() < 1e-12);
        assert!(
            (geom::dot(axis, frame.z).abs() - 1.).abs() < 1e-12,
            "face {face}: {axis:?}"
        );
        let fp = reading
            .fingerprints
            .features
            .iter()
            .find(|f| f.family == "fillets" && f.faces == vec![face])
            .unwrap();
        assert!((geom::dot(fp.axis.unwrap(), frame.z).abs() - 1.).abs() < 1e-12);
        assert!(
            reading.features.defining["blends"]
                .iter()
                .all(|d| !d.contains(&face))
        );
    }
}
