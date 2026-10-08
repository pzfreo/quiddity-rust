//! Rectangular pad evidence on the parts of Python's own pad tests (`tests/test_pad_attribution.py`,
//! captured as STEP) and its golden fixture: the captured replay compares records only, so the
//! five faces each pad is defined by are checked here, as Python's tests check them, and the
//! tolerances JSON cannot carry (NaN, infinity) are asked directly.

use std::path::Path;

use quiddity::features::Context;
use quiddity::features::graph::normal;
use quiddity::features::pads::{PadOptions, discover};
use quiddity::{Part, read_step_file};

fn part(path: &str) -> Part {
    read_step_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

/// `_assert_signed_five_face_evidence`: the one pad's defining faces are its top, facing along
/// the pad's direction at its terminal level, and four walls normal to it, one each way along
/// both transverse axes.
fn assert_five_face_evidence(part: &Part, axis: usize, direction: f64) {
    let found = discover(&Context::new(part), &PadOptions::default()).expect("pad evidence");
    let [pad] = &found[..] else {
        panic!("{} pads", found.len());
    };
    assert_eq!(pad.defining.len(), 5);
    let normals: Vec<_> = pad
        .defining
        .iter()
        .map(|&f| (f, normal(part, f).unwrap()))
        .collect();
    let tops: Vec<_> = normals
        .iter()
        .filter(|(_, n)| n[axis].abs() >= 0.99)
        .collect();
    let [(top, n)] = tops[..] else {
        panic!("tops {tops:?}");
    };
    assert!(n[axis] * direction >= 0.99, "top {top} faces {n:?}");
    let terminal = if direction > 0.0 {
        part.face_bounds(*top).max[axis]
    } else {
        part.face_bounds(*top).min[axis]
    };
    let bounds = [
        (pad.record.x0, pad.record.x1),
        (pad.record.y0, pad.record.y1),
        (pad.record.z0, pad.record.z1),
    ];
    let record_terminal = if direction > 0.0 {
        bounds[axis].1
    } else {
        bounds[axis].0
    };
    assert!((terminal - record_terminal).abs() < 1e-6);
    let mut headings: Vec<(usize, bool)> = normals
        .iter()
        .filter(|(f, _)| f != top)
        .map(|(_, n)| {
            let along = (0..3)
                .find(|&i| n[i].abs() >= 0.99)
                .expect("a principal wall");
            assert_ne!(along, axis);
            (along, n[along] > 0.0)
        })
        .collect();
    headings.sort_unstable();
    headings.dedup();
    assert_eq!(headings.len(), 4, "walls {normals:?}");
}

#[test]
fn a_sharp_pad_owns_its_top_and_four_walls() {
    // `_pad()`: Box(80, 60, 10) + Pos(0, 0, 7) * Box(30, 20, 4).
    assert_five_face_evidence(
        &part("tests/fixtures/captured/93d93a4ff5a93d06.step.gz"),
        2,
        1.0,
    );
}

#[test]
fn a_corner_blended_pad_owns_its_top_and_four_walls() {
    // `_blended_pad()`: the same pad with its four vertical corners filleted; the blends are not
    // defining faces.
    assert_five_face_evidence(
        &part("tests/fixtures/captured/4f66ce9b6b1173f3.step.gz"),
        2,
        1.0,
    );
}

#[test]
fn the_golden_fixture_has_its_one_pad() {
    let part = part("tests/fixtures/golden_plates_pads_levels_and_slanted_steps.step");
    let found = quiddity::recognise_rectangular_pads(&part, &PadOptions::default()).unwrap();
    let json = serde_json::to_value(&found).unwrap();
    assert_eq!(
        json,
        serde_json::json!([{"x0": 18.0, "x1": 42.0, "y0": -9.0, "y1": 9.0, "z0": 12.0,
                            "z1": 28.0, "axis": "z", "direction": 1}])
    );
    assert_five_face_evidence(&part, 2, 1.0);
}

/// `test_existing_invalid_tolerance_behavior_remains_empty`: NaN and infinity (which the capture
/// cannot record) and a negative tolerance find nothing, on both paths.
#[test]
fn invalid_tolerances_find_nothing() {
    let part = part("tests/fixtures/captured/93d93a4ff5a93d06.step.gz");
    for tol in [-0.1, f64::NAN, f64::INFINITY] {
        let opts = PadOptions { tol: Some(tol) };
        assert_eq!(
            quiddity::recognise_rectangular_pads(&part, &opts).unwrap(),
            vec![],
            "tol {tol}"
        );
        assert!(discover(&Context::new(&part), &opts).unwrap().is_empty());
    }
}
