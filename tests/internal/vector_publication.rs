//! #92: reconstruct published geometry with the provenance frame withheld.
use super::common;
use quiddity::Part;
use quiddity::correspondence;
use quiddity::kernel::geom::{V3, dot};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use quiddity::recognition::{self, Record};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn vector(v: &Value) -> V3 {
    match v.as_str() {
        Some("x") => [1., 0., 0.],
        Some("y") => [0., 1., 0.],
        Some("z") => [0., 0., 1.],
        _ => serde_json::from_value(v.clone()).expect("file vector"),
    }
}
fn number(v: &Value) -> f64 {
    v.as_f64().expect("coordinate")
}
fn origin(p: &Value) -> V3 {
    p.get("origin").map_or([0.; 3], vector)
}
fn lifted(o: V3, coordinates: &[(f64, V3)]) -> V3 {
    std::array::from_fn(|i| o[i] + coordinates.iter().map(|(s, d)| s * d[i]).sum::<f64>())
}
fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b}");
}
fn moved(degrees: f64) -> Placement {
    let (s, c) = degrees.to_radians().sin_cos();
    [[c, -s, 0., 12.345], [s, c, 0., -7.891], [0., 0., 1., 3.456]]
}
fn document(part: &Part) -> recognition::RecognitionDocument {
    let found = correspondence::recognise(part);
    let mut json = serde_json::to_value(recognition::document(&found, part.faces.len())).unwrap();
    // The geometric decoder below has access only to the records.
    json.as_object_mut().unwrap().remove("frame");
    json["frame"] = json!({"status":"caller-space"});
    serde_json::from_value(json).unwrap()
}
fn slot_endpoints(part: &Part, r: &Record) -> [V3; 2] {
    let p = json!(r.parameters);
    let o = origin(&p);
    let w = vector(&p["width_axis"]);
    let l = vector(&p["long_axis"]);
    let d = p.get("depth_axis").map(vector).unwrap_or_else(|| {
        let wi = w.iter().position(|x| x.abs() == 1.).unwrap();
        let li = l.iter().position(|x| x.abs() == 1.).unwrap();
        let mut d = [0.; 3];
        d[3 - wi - li] = 1.;
        d
    });
    for v in [w, l, d] {
        near(dot(v, v), 1., 1e-12);
    }
    let ends = [number(&p["lo"]), number(&p["hi"])].map(|s| {
        lifted(
            o,
            &[
                (number(&p["w_center"]), w),
                (s, l),
                ((number(&p["d_lo"]) + number(&p["d_hi"])) / 2., d),
            ],
        )
    });
    // The two defining slot walls independently bound all three published coordinates.
    for (dir, expected) in [
        (l, [dot(ends[0], l), dot(ends[1], l)]),
        (
            w,
            [
                dot(o, w) + number(&p["w_center"]) - number(&p["width"]) / 2.,
                dot(o, w) + number(&p["w_center"]) + number(&p["width"]) / 2.,
            ],
        ),
        (
            d,
            [
                dot(o, d) + number(&p["d_lo"]),
                dot(o, d) + number(&p["d_hi"]),
            ],
        ),
    ] {
        let actual = part.extent_along(&r.faces, dir);
        near(actual.0, expected[0], 0.0051);
        near(actual.1, expected[1], 0.0051);
    }
    ends
}
#[test]
fn golden_slots_and_levels_reconstruct_on_faces_and_move_rigidly() {
    let path = common::fixtures().join("golden_oriented_slots.step");
    let base = read_step_file_placed(&path, &IDENTITY).unwrap();
    let doc = document(&base);
    let slots: Vec<_> = doc.records.iter().filter(|r| r.family == "slots").collect();
    assert_eq!(slots.len(), 3);
    for r in &slots {
        let l = vector(&r.parameters["long_axis"]);
        near(l[0].abs(), 30_f64.to_radians().cos(), 1e-10);
        near(l[1].abs(), 0.5, 1e-10);
        slot_endpoints(&base, r);
    }
    for degrees in [0., 30., -30., 60., 1.] {
        let placement = if degrees == 0. {
            IDENTITY
        } else {
            moved(degrees)
        };
        let part = read_step_file_placed(&path, &placement).unwrap();
        let copy = document(&part);
        let found: Vec<_> = copy
            .records
            .iter()
            .filter(|r| r.family == "slots")
            .collect();
        assert_eq!(found.len(), 3, "at {degrees}");
        for r in found {
            let points = slot_endpoints(&part, r);
            let original = slots
                .iter()
                .find(|s| s.faces == r.faces)
                .expect("same slot faces");
            let expected = slot_endpoints(&base, original).map(|p| {
                std::array::from_fn::<_, 3, _>(|i| {
                    (0..3).map(|j| placement[i][j] * p[j]).sum::<f64>() + placement[i][3]
                })
            });
            for p in points {
                assert!(
                    expected.iter().any(|q| (0..3)
                        .map(|i| (p[i] - q[i]).powi(2))
                        .sum::<f64>()
                        .sqrt()
                        < 0.011),
                    "endpoint {p:?} at {degrees}"
                );
            }
        }
        let levels: Vec<_> = copy
            .records
            .iter()
            .filter(|r| r.family == "step_levels")
            .collect();
        assert_eq!(levels.len(), 6);
        for r in levels {
            let p = json!(r.parameters);
            let o = origin(&p);
            let h = p.get("height_axis").map(vector).unwrap_or([0., 0., 1.]);
            let x = p.get("x_axis").map(vector).unwrap_or([1., 0., 0.]);
            let y = p.get("y_axis").map(vector).unwrap_or([0., 1., 0.]);
            let middle = |key: &str| (number(&p[key][0]) + number(&p[key][1])) / 2.;
            let point = lifted(
                o,
                &[
                    (number(&p["z"]), h),
                    (middle("x_span"), x),
                    (middle("y_span"), y),
                ],
            );
            let actual = part.extent_along(&r.faces, h);
            near(actual.0, dot(point, h), 1e-7);
            near(actual.1, dot(point, h), 1e-7);
            for (axis, key) in [(x, "x_span"), (y, "y_span")] {
                let span = part.extent_along(&r.faces, axis);
                near(span.0, dot(o, axis) + number(&p[key][0]), 1e-7);
                near(span.1, dot(o, axis) + number(&p[key][1]), 1e-7);
            }
        }
    }
}
#[test]
fn fcadsample_riser_height_and_end_sense_are_file_geometry() {
    let path = common::fixtures().join("fcadsample.step");
    for degrees in [0., 30., -30., 60., 1.] {
        let placement = if degrees == 0. {
            IDENTITY
        } else {
            moved(degrees)
        };
        let part = read_step_file_placed(&path, &placement).unwrap();
        let doc = document(&part);
        let risers: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "risers")
            .collect();
        assert_eq!(risers.len(), 1);
        let r = risers[0];
        assert_eq!(r.faces, [3]);
        let p = json!(r.parameters);
        let o = origin(&p);
        let h = vector(&p["height_axis"]);
        let (lo, hi) = (number(&p["z_lo"]), number(&p["z_hi"]));
        let low = lifted(o, &[(lo, h)]);
        let high = lifted(o, &[(hi, h)]);
        let actual = part.extent_along(&r.faces, h);
        near(actual.0, dot(low, h), 1e-7);
        near(actual.1, dot(high, h), 1e-7);
        near(hi - lo, 21., 1e-7);
        assert_eq!(p["lo_at_envelope"], true);
        assert_eq!(p["hi_at_envelope"], false);
        if degrees != 0. {
            assert!(p["height_axis"].is_array());
        }
        assert!(p["positions"].as_array().unwrap().is_empty());
    }
}
fn paths(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            paths(&p, out);
        } else if p.to_str().is_some_and(|p| {
            [".step", ".stp", ".step.gz", ".stp.gz"]
                .iter()
                .any(|s| p.ends_with(s))
        }) {
            out.push(p);
        }
    }
}
fn no_local(v: &Value) {
    match v {
        Value::Object(o) => {
            for (k, v) in o {
                assert_ne!(k, "local", "published local key");
                no_local(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                no_local(v);
            }
        }
        _ => {}
    }
}
#[test]
fn all_fixture_and_corpus_publications_have_no_local_key() {
    let mut files = Vec::new();
    paths(&common::fixtures(), &mut files);
    if let Some(dir) = common::corpus_dir() {
        paths(&dir, &mut files);
    } else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
    }
    let counts = common::parallel::map(&files, |p| {
        // Refused STEP inputs have no publication; every successfully loaded part is checked.
        let Ok(part) = quiddity::read_step_file(p) else {
            return 0;
        };
        let found = correspondence::recognise(&part);
        no_local(&serde_json::to_value(&found).unwrap());
        validate_boxes(&serde_json::to_value(&found).unwrap(), &part);
        no_local(&serde_json::to_value(recognition::document(&found, part.faces.len())).unwrap());
        1
    });
    assert!(counts.iter().sum::<usize>() >= 100);
    no_local(&json!({"endpoint_scopes":["local","envelope"]}));
}

pub(super) fn validate_boxes(v: &Value, part: &Part) {
    let mut seen = std::collections::BTreeSet::new();
    check_boxes(v, part, &mut seen);
}
fn check_boxes(v: &Value, part: &Part, seen: &mut std::collections::BTreeSet<String>) {
    match v {
        Value::Object(o) => {
            if o.contains_key("axes")
                && o.contains_key("bounds")
                && o.contains_key("origin")
                && seen.insert(v.to_string())
            {
                let origin = vector(&o["origin"]);
                let axes: Vec<_> = o["axes"].as_array().unwrap().iter().map(vector).collect();
                let fits = part.solids.iter().any(|solid| {
                    axes.iter().enumerate().all(|(i, &d)| {
                        let (lo, hi) = part.extent_along(&solid.faces, d);
                        let shift = dot(origin, d);
                        (lo - shift - number(&o["bounds"][i][0])).abs() <= 3e-6
                            && (hi - shift - number(&o["bounds"][i][1])).abs() <= 3e-6
                    })
                });
                assert!(
                    fits,
                    "published box does not reconstruct a body's extrema: {v}"
                );
            }
            for (key, v) in o {
                if let Some(a) = v.as_array()
                    && ((key == "body_key" && a.len() == 8)
                        || (key == "body_bounds" && a.len() == 6))
                {
                    let intervals: Vec<_> = (0..3)
                        .map(|i| {
                            if key == "body_key" {
                                json!([a[i], a[i + 3]])
                            } else {
                                json!([a[2 * i], a[2 * i + 1]])
                            }
                        })
                        .collect();
                    let file_box =
                        json!({"origin":[0.,0.,0.],"axes":["x","y","z"],"bounds":intervals});
                    check_boxes(&file_box, part, seen);
                }
                check_boxes(v, part, seen);
            }
        }
        Value::Array(a) => {
            for v in a {
                check_boxes(v, part, seen);
            }
        }
        _ => {}
    }
}

/// An axial gauge's free roll changes the selected height context. Both representatives must
/// nevertheless reconstruct the actual cylinder's support planes without provenance.
#[test]
fn axial_gauge_riser_supports_are_valid_under_free_roll() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let [x, y, z] = [1., 2., 3.].map(|v| v / 14_f64.sqrt());
    let (s, c) = 37_f64.to_radians().sin_cos();
    let t = 1. - c;
    let generic = [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y, 123.456],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x, -78.9],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c, 41.3],
    ];
    for (name, face) in [
        ("gramel/GRM-03_thumbwheel_drive_screw.step", 0),
        ("gramel/GRM-05_depth_lock_bolt.step", 2),
    ] {
        for placement in [IDENTITY, moved(90.), generic] {
            let part = read_step_file_placed(&dir.join(name), &placement).unwrap();
            let doc = document(&part);
            let r = doc
                .records
                .iter()
                .find(|r| r.family == "risers" && r.faces == [face])
                .unwrap();
            let p = json!(r.parameters);
            let o = origin(&p);
            let h = p.get("height_axis").map(vector).unwrap_or([0., 0., 1.]);
            let actual = part.extent_along(&r.faces, h);
            near(actual.0, dot(o, h) + number(&p["z_lo"]), 1e-6);
            near(actual.1, dot(o, h) + number(&p["z_hi"]), 1e-6);
        }
    }
}

#[test]
fn cgb217_turning_stations_preserve_the_recognised_context() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = quiddity::read_step_file(&dir.join("cadgenbench_inputs/cgb217.step.gz")).unwrap();
    let frame = quiddity::frames::infer_part_frame(&part).unwrap();
    let working = part.transformed(&frame.normalisation()).unwrap();
    let raw = quiddity::features::recognise(&working);
    let index = raw.defining["turned_steps"]
        .iter()
        .position(|f| f == &[314])
        .unwrap();
    let source = &raw.turned_steps[index];
    near(*source.hi - *source.lo, 5., 1e-9);
    let found = correspondence::recognise(&part);
    let doc = recognition::document(&found, part.faces.len());
    let record = doc
        .records
        .iter()
        .find(|r| r.family == "turned_steps" && r.faces == [314])
        .unwrap();
    let p = json!(record.parameters);
    near(number(&p["lo"]), *source.lo, 1e-12);
    near(number(&p["hi"]), *source.hi, 1e-12);
    let o = origin(&p);
    let d = vector(&p["axis"]);
    near(dot(o, d) + number(&p["lo"]), -30., 0.0005);
    near(dot(o, d) + number(&p["hi"]), -25., 0.0005);
    let caller = quiddity::features::recognise(&part);
    let index = caller.defining["turned_steps"]
        .iter()
        .position(|f| f == &[314])
        .unwrap();
    near(*caller.turned_steps[index].lo, -29., 1e-9);
    near(*caller.turned_steps[index].hi, -25., 1e-9);
}

#[test]
fn a_flat_axis_line_needs_no_provenance_even_with_a_tilted_stock_axis() {
    use quiddity::features::{
        Features,
        flats::Flat,
        placed::{Grid, Placed},
    };
    use quiddity::framed_records::{Rigid, map_features};
    let d = quiddity::kernel::geom::unit([1., 0.02, 0.03]).unwrap();
    let raw = Features {
        flats: vec![Flat {
            axis: 'x',
            across: 4.,
            at: Placed::new([0.; 3], Grid::Exact),
            axis_line: Placed::new((2., 3.), Grid::Exact),
            stock_span: Placed::new((5., 9.), Grid::Exact),
            axis_direction: d,
        }],
        ..Features::default()
    };
    let motion = Rigid::from_placement(&moved(30.));
    let mapped = map_features(raw, &motion);
    let p = serde_json::to_value(&mapped.features).unwrap()["flats"][0].clone();
    let o = origin(&p);
    let a = vector(&p["axis"]);
    let run = vector(&p["axis_direction"]);
    let axes = p["section_axes"].as_array().unwrap();
    let q = lifted(
        o,
        &[
            (number(&p["axis_line"][0]), vector(&axes[0])),
            (number(&p["axis_line"][1]), vector(&axes[1])),
        ],
    );
    let relative = std::array::from_fn(|i| q[i] - o[i]);
    let q = lifted(q, &[(-dot(relative, run) / dot(a, run), a)]);
    let expected = motion.point([-(2. * d[1] + 3. * d[2]) / d[0], 2., 3.]);
    for i in 0..3 {
        near(q[i], expected[i], 1e-12);
    }
    let shift = dot(motion.translation, run);
    near(number(&p["stock_span"][0]), 5. + shift, 1e-12);
    near(number(&p["stock_span"][1]), 9. + shift, 1e-12);
}

#[test]
fn vector_snaps_share_the_coordinate_floor_across_the_whole_basis() {
    use quiddity::features::{
        Features,
        fillets::Fillet,
        placed::{Grid, Placed},
    };
    use quiddity::frames::{FrameGauge, PartFrame};
    let part = quiddity::read_step_file(&common::fixtures().join("filleted_plate_with_holes.step"))
        .unwrap();
    let bounds = part.bounds();
    let reach = (0..3)
        .map(|i| bounds.min[i].abs().max(bounds.max[i].abs()).powi(2))
        .sum::<f64>()
        .sqrt();
    for (factor, snapped) in [(0.5, true), (0.9, false)] {
        let radians = factor * quiddity::kernel::geom::COORD_FLOOR / reach;
        let (s, c) = radians.sin_cos();
        let k = 1. / 3_f64.sqrt();
        let t = (1. - c) / 3.;
        let rows = [
            [t + c, t - s * k, t + s * k],
            [t + s * k, t + c, t - s * k],
            [t - s * k, t + s * k, t + c],
        ];
        let frame = PartFrame {
            origin: [0.; 3],
            x: rows.map(|r| r[0]),
            y: rows.map(|r| r[1]),
            z: rows.map(|r| r[2]),
            gauge: FrameGauge::Full,
        };
        let raw = Features {
            fillets: ['x', 'y', 'z']
                .map(|axis| Fillet {
                    axis,
                    radius: 1.,
                    at: Placed::new([0.; 3], Grid::Exact),
                    turned: false,
                    side: "convex",
                })
                .into(),
            ..Features::default()
        };
        let mapped = quiddity::framed_records::to_file(raw, &frame, &part);
        let json = serde_json::to_value(&mapped.features).unwrap();
        for r in json["fillets"].as_array().unwrap() {
            assert_eq!(r["axis"].is_string(), snapped);
        }
    }
}
