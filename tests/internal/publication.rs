//! Framed records are published in the file's coordinates on their grids (#69), and a frame
//! aligned with the file's axes up to round-off maps exactly (#70).

use super::common;

use quiddity::correspondence::{self, Recognition};
use quiddity::features::{self, placed::audit};
use quiddity::framed_records::{self, Mapped, Rigid, map_features};
use quiddity::frames::{RecordFrame, infer_part_frame};
use quiddity::kernel::geom::Surface;
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use serde_json::Value;

/// The rotation by *radians* about *axis* (unit), then *translation*.
fn motion(axis: [f64; 3], radians: f64, translation: [f64; 3]) -> Placement {
    let [x, y, z] = axis;
    let (s, c) = radians.sin_cos();
    let t = 1.0 - c;
    [
        [
            t * x * x + c,
            t * x * y - s * z,
            t * x * z + s * y,
            translation[0],
        ],
        [
            t * x * y + s * z,
            t * y * y + c,
            t * y * z - s * x,
            translation[1],
        ],
        [
            t * x * z - s * y,
            t * y * z + s * x,
            t * z * z + c,
            translation[2],
        ],
    ]
}

fn golden() -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(common::fixtures())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("golden_") && n.ends_with(".step"))
        })
        .collect();
    paths.sort();
    paths
}

/// The paths at which *a* and *b* differ by more than 1e-7: less than any published grid's step
/// (1e-6 mm at the finest), more than the round-off a motion leaves in an unrounded value.
fn differences(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() > 1e-7 {
                out.push(format!("{path}: {x} vs {y}"));
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                differences(p, q, &format!("{path}[{i}]"), out);
            }
        }
        (Value::Object(x), Value::Object(y)) if x.len() == y.len() => {
            for (k, p) in x {
                match y.get(k) {
                    Some(q) => differences(p, q, &format!("{path}.{k}"), out),
                    None => out.push(format!("{path}.{k} missing")),
                }
            }
        }
        _ if a == b => {}
        _ => out.push(format!("{path}: {a} vs {b}")),
    }
}

/// *v* with each number on a 1e-6 grid and each list of records in a canonical order: a
/// record's content, whatever order ties among equal records leave.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Number(n) => serde_json::json!((n.as_f64().unwrap() * 1e6).round() / 1e6 + 0.0),
        Value::Array(items) => {
            let mut items: Vec<Value> = items.iter().map(canonical).collect();
            if items.iter().all(Value::is_object) {
                items.sort_by_key(|i| i.to_string());
            }
            Value::Array(items)
        }
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn records(r: &Recognition) -> Value {
    let mut v = serde_json::to_value(&r.features).unwrap();
    v.as_object_mut().unwrap().remove("fingerprints");
    v
}

/// Every corpus part's published positions are on their grids; carried as found in the frame
/// (what fingerprints read, and what was published before #69), most are not.
#[test]
fn published_positions_are_on_their_grids_on_every_corpus_part() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        return;
    };
    let files: Vec<String> = common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_string())
        .collect();
    let counts = common::parallel::map(&files, |name| {
        let part = read_step_file_placed(&dir.join(name), &IDENTITY).ok()?;
        let frame = infer_part_frame(&part).ok()?;
        let working = part.transformed(&frame.normalisation()).ok()?;
        let Mapped {
            features,
            fingerprinted,
            ..
        } = framed_records::to_file(features::recognise(&working), &frame);
        let before = audit::count(|| drop(serde_json::to_value(&fingerprinted).unwrap()));
        let after = audit::count(|| drop(serde_json::to_value(&features).unwrap()));
        Some((name.clone(), before, after))
    });
    let (mut checked, mut before, mut after) = (0, 0, 0);
    let mut problems = Vec::new();
    for (name, (n, b), (m, a)) in counts.into_iter().flatten() {
        assert_eq!(n, m, "{name}: the same positions are published");
        checked += m;
        before += b;
        after += a;
        if a > 0 {
            problems.push(format!("{name}: {a} of {m} positions off their grid"));
        }
    }
    eprintln!(
        "published positions: {checked}; off their grid carried as found {before}, published {after}"
    );
    assert!(checked > 10_000 && before > 0, "{checked} {before}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A moved copy publishes the original's records carried by the motion: a quarter turn and an
/// off-grid translation, carried as unrounded values and published once, give exactly what
/// recognising the moved part gives.
#[test]
fn a_moved_copy_publishes_the_carried_records() {
    let exact: Placement = [
        [0.0, -1.0, 0.0, 12.3456],
        [1.0, 0.0, 0.0, -7.0004],
        [0.0, 0.0, 1.0, 0.5],
    ];
    let mut compared = 0;
    let mut problems = Vec::new();
    for path in golden() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let unmoved = correspondence::recognise(&read_step_file_placed(&path, &IDENTITY).unwrap());
        let moved = correspondence::recognise(&read_step_file_placed(&path, &exact).unwrap());
        let (RecordFrame::Inferred(f0), RecordFrame::Inferred(f1)) = (&unmoved.frame, &moved.frame)
        else {
            continue;
        };
        if f0.gauge != quiddity::frames::FrameGauge::Full || f1.gauge != f0.gauge {
            continue;
        }
        let carried = map_features(unmoved.features.clone(), &Rigid::from_placement(&exact));
        let mut expected = serde_json::to_value(&carried.features).unwrap();
        let mut got = serde_json::to_value(&moved.features).unwrap();
        // Step levels and risers are body evidence read along the frame's z, often left in the
        // frame; risers' count already differs under this motion before #69
        // (golden_triangular_and_hex_pockets: 2 unmoved, 1 moved, at 9c3fd85). Not compared.
        for v in [&mut expected, &mut got] {
            v["step_levels"] = Value::Null;
            v["risers"] = Value::Null;
        }
        // A field left in the frame is the same in both, not carried: not compared.
        for id in unmoved.local.keys().chain(moved.local.keys()) {
            let (family, n) = id.rsplit_once('/').unwrap();
            let n: usize = n.parse().unwrap();
            for v in [&mut expected, &mut got] {
                if let Some(record) = v.get_mut(family).and_then(|f| f.get_mut(n)) {
                    *record = Value::Null;
                }
            }
        }
        compared += 1;
        let mut found = Vec::new();
        differences(&expected, &got, &name, &mut found);
        problems.extend(found);
    }
    assert!(
        compared >= 9,
        "only {compared} golden parts have a full frame"
    );
    assert!(problems.is_empty(), "differ:\n{}", problems.join("\n"));
}

/// A part aligned with the file's axes up to round-off is in an exact signed permutation of
/// them, and its records come back in the file's axes: a 1e-10 rad rotation leaves the records
/// as the unrotated part's (to 1e-6 mm, in any order ties among equal records leave).
#[test]
fn a_tiny_rotation_is_mapped_as_the_aligned_part() {
    let mut compared = 0;
    for path in golden() {
        let unmoved = correspondence::recognise(&read_step_file_placed(&path, &IDENTITY).unwrap());
        let tilted = motion([0.6, 0.8, 0.0], 1e-10, [0.0; 3]);
        let moved = correspondence::recognise(&read_step_file_placed(&path, &tilted).unwrap());
        let (RecordFrame::Inferred(f0), RecordFrame::Inferred(f1)) = (&unmoved.frame, &moved.frame)
        else {
            continue;
        };
        let exact = |v: [f64; 3]| v.iter().all(|c| [0.0, 1.0, -1.0].contains(c));
        if ![f0.x, f0.y, f0.z].into_iter().all(exact) {
            continue; // not aligned with the file's axes: nothing to snap
        }
        assert!(
            [f1.x, f1.y, f1.z].into_iter().all(exact),
            "{path:?}: {f1:?}"
        );
        // A free gauge (a roll about a turned part's axis) may take another representative.
        if f0.gauge == quiddity::frames::FrameGauge::Full {
            assert_eq!((f0.x, f0.y, f0.z), (f1.x, f1.y, f1.z), "{path:?}");
            assert_eq!(unmoved.local, moved.local, "{path:?}");
            assert_eq!(
                canonical(&records(&unmoved)),
                canonical(&records(&moved)),
                "{path:?}"
            );
        }
        compared += 1;
    }
    assert!(compared >= 10, "only {compared} golden parts have a frame");
}

/// sm-hanger's frame is aligned with the file's axes up to 2.4e-9: it is snapped, so its
/// fillets name the file's axis their cylinder runs along, and nothing but the step levels
/// (read along the frame's z by specification, here the file's x) is left local.
#[test]
fn sm_hanger_is_mapped_in_the_files_axes() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        return;
    };
    let part = read_step_file_placed(&dir.join("ttt_inputs/sm-hanger.step"), &IDENTITY).unwrap();
    let r = correspondence::recognise(&part);
    let RecordFrame::Inferred(frame) = &r.frame else {
        panic!("{:?}", r.frame);
    };
    assert_eq!(
        [frame.x, frame.y, frame.z],
        [[0.0, 0.0, 1.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]]
    );
    for (id, paths) in &r.local {
        assert!(id.starts_with("step_levels/"), "{id}: {paths:?}");
    }
    assert_eq!(r.features.fillets.len(), 6);
    let defining = &r.features.defining["fillets"];
    for (fillet, faces) in r.features.fillets.iter().zip(defining) {
        let Surface::Cylinder { frame: cyl, .. } = &part.faces[faces[0]].surface else {
            panic!("a fillet is a cylinder");
        };
        let w = (0..3)
            .max_by(|&a, &b| cyl.z[a].abs().total_cmp(&cyl.z[b].abs()))
            .unwrap();
        assert_eq!(fillet.axis, ['x', 'y', 'z'][w], "{fillet:?}");
    }
}
