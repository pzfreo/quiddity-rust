//! Test-only additive schema experiment, not a proposed public record format. The known rigid
//! motion supplies the coordinate frame; actual moved support planes independently check it.
//! Adding the frame preserves local fields, but existing readers ignore it and interpret axis
//! letters as caller axes. Publishing this would therefore require a consumer/API migration.
use super::{common, golden_paths};
use crate::motion_sweep::rotation;
use quiddity::Part;
use quiddity::features::{Context, plates, pockets};
use quiddity::kernel::geom::{self, Curve, Surface, V3};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
struct FrameTagged<R> {
    #[serde(flatten)]
    record: R,
    coordinate_frame: Placement,
}

fn axis_index(axis: char) -> usize {
    ['x', 'y', 'z'].iter().position(|&a| a == axis).unwrap()
}

fn direction(at: &Placement, axis: char) -> V3 {
    let i = axis_index(axis);
    [0, 1, 2].map(|k| at[k][i])
}

fn point(at: &Placement, p: V3) -> V3 {
    [0, 1, 2].map(|i| at[i][3] + (0..3).map(|j| at[i][j] * p[j]).sum::<f64>())
}

fn plane_normal(part: &Part, face: usize) -> V3 {
    let Surface::Plane { frame } = part.faces[face].surface else {
        panic!("a plate/wall is planar");
    };
    frame.z
}

fn plane_span(part: &Part, faces: &[usize], normal: V3) -> (f64, f64) {
    faces
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &f| {
            assert!((geom::dot(plane_normal(part, f), normal).abs() - 1.).abs() < 1e-9);
            let Surface::Plane { frame } = part.faces[f].surface else {
                unreachable!()
            };
            let offset = geom::dot(frame.origin, normal);
            (lo.min(offset), hi.max(offset))
        })
}

fn plate_centre(p: &plates::Plate) -> V3 {
    let axis = axis_index(p.axis);
    let other: Vec<_> = (0..3).filter(|&i| i != axis).collect();
    let mut c = [0.; 3];
    c[axis] = (p.lo + p.hi) / 2.;
    c[other[0]] = p.u;
    c[other[1]] = p.v;
    c
}

fn plate_records(part: &Part) -> BTreeMap<Vec<usize>, plates::Plate> {
    plates::discover_verified(&Context::new(part), &Default::default())
        .unwrap()
        .into_iter()
        .map(|mut o| {
            o.defining.sort_unstable();
            (o.defining, o.record)
        })
        .collect()
}

#[test]
fn plate_frame_metadata_preserves_geometry_but_changes_axis_interpretation() {
    let translation = [123.456, -78.9, 41.3];
    let motions = [
        rotation([0., 0., 1.], 30., translation),
        rotation([1., 0., 0.], 30., translation),
    ];
    let mut total = 0;
    let mut raw_counts = [0; 2];
    let mut legacy_wrong = [0; 2];
    let mut preserved_z_roll = 0;
    let mut analytic_centres = 0;
    for path in golden_paths() {
        let baseline = plate_records(&read_step_file_placed(&path, &IDENTITY).unwrap());
        total += baseline.len();
        if baseline.is_empty() {
            continue;
        }
        for (pose, at) in motions.iter().enumerate() {
            let part = read_step_file_placed(&path, at).unwrap();
            let raw = plate_records(&part);
            raw_counts[pose] += raw.len();
            assert!(raw.keys().all(|f| baseline.contains_key(f)));
            for (faces, before) in &baseline {
                let prototype = FrameTagged {
                    record: before.clone(),
                    coordinate_frame: *at,
                };
                let encoded = serde_json::to_value(&prototype).unwrap();
                let decoded: FrameTagged<plates::Plate> =
                    serde_json::from_value(encoded.clone()).unwrap();
                assert_eq!(decoded.record, *before);
                assert_eq!(decoded.coordinate_frame, *at);
                let mut local = encoded.clone();
                local.as_object_mut().unwrap().remove("coordinate_frame");
                assert_eq!(local, serde_json::to_value(before).unwrap());
                let normal = direction(&decoded.coordinate_frame, decoded.record.axis);
                let (lo, hi) = plane_span(&part, faces, normal);
                let shift = geom::dot(translation, normal);
                assert!((lo - (before.lo + shift)).abs() < 1e-7);
                assert!((hi - (before.hi + shift)).abs() < 1e-7);
                assert!(((hi - lo) - before.thickness()).abs() < 1e-7);

                // The existing public reader accepts the additive key but silently ignores it.
                let legacy: plates::Plate = serde_json::from_value(encoded).unwrap();
                assert_eq!(legacy, *before);
                let caller_axis = direction(&IDENTITY, legacy.axis);
                if geom::dot(normal, caller_axis).abs() < 1. - 1e-9 {
                    legacy_wrong[pose] += 1;
                    assert!(!raw.contains_key(faces));
                } else {
                    let after = &raw[faces];
                    assert!((after.thickness() - before.thickness()).abs() < 1e-8);
                    // Curved-edge centres use numerical boundary integration. Check the
                    // centre additionally where straight boundaries give analytic moments;
                    // normals, face identity and plane separation are checked for every plate.
                    if faces.iter().all(|&f| {
                        part.face_edges(f)
                            .iter()
                            .all(|&e| matches!(part.edges[e].curve, Curve::Line { .. }))
                    }) {
                        assert!(
                            geom::dist(plate_centre(after), point(at, plate_centre(before))) < 1e-8
                        );
                        analytic_centres += 1;
                    }
                    if pose == 0 {
                        assert_eq!(before.axis, 'z');
                        preserved_z_roll += 1;
                    }
                }
            }
        }
    }
    assert_eq!(total, 16);
    assert_eq!(raw_counts, [5, 1]);
    assert_eq!(legacy_wrong, [11, 15]);
    assert_eq!(preserved_z_roll, 5);
    assert!(analytic_centres > 0);
    eprintln!(
        "PLATE PROTOTYPE: baseline {total}; raw Z30/X30 {raw_counts:?}; frame-tagged geometry 16/16; legacy wrong {legacy_wrong:?}; preserved Z30 roll {preserved_z_roll}"
    );
}

#[test]
fn pocket_frame_metadata_requires_frame_aware_width_and_long_axes() {
    let path = common::fixtures().join("golden_blind_pockets_and_pocket_patterns.step");
    let part = read_step_file_placed(&path, &IDENTITY).unwrap();
    let before = pockets::discover_verified(&Context::new(&part))
        .unwrap()
        .into_iter()
        .find(|o| {
            o.record.width_axis != 'z'
                && o.record.long_axis != 'z'
                && o.defining.len() == 2
                && o.defining.iter().all(|&f| {
                    geom::dot(
                        plane_normal(&part, f),
                        direction(&IDENTITY, o.record.width_axis),
                    )
                    .abs()
                        > 1. - 1e-9
                })
        })
        .expect("the golden has opposed planar pocket walls and a Z depth axis");
    let at = rotation([0., 0., 1.], 30., [123.456, -78.9, 41.3]);
    let moved = read_step_file_placed(&path, &at).unwrap();
    let prototype = FrameTagged {
        record: before.record.clone(),
        coordinate_frame: at,
    };
    let encoded = serde_json::to_value(&prototype).unwrap();
    let aware: FrameTagged<pockets::Pocket> = serde_json::from_value(encoded.clone()).unwrap();
    let normal = direction(&aware.coordinate_frame, aware.record.width_axis);
    let (lo, hi) = plane_span(&moved, &before.defining, normal);
    assert!((hi - lo - before.record.width).abs() < 1e-7);
    let legacy: pockets::Pocket = serde_json::from_value(encoded).unwrap();
    assert_eq!(legacy, before.record);
    assert!(geom::dot(normal, direction(&IDENTITY, legacy.width_axis)).abs() < 1. - 1e-9);
    assert_eq!(direction(&at, 'z'), [0., 0., 1.]);
    let found = pockets::discover_verified(&Context::new(&moved)).unwrap();
    assert!(found.iter().all(|o| o.defining != before.defining));
    eprintln!(
        "POCKET PROTOTYPE: Z30 preserves depth normal; actual opposed walls rotate; additive metadata works only for a frame-aware reader"
    );
}
