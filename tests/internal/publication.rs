//! Framed records are published in the file's coordinates on their grids (#69), and a frame
//! aligned with the file's axes up to round-off is published in the file's axes (#70).

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

/// The paths at which *a* and *b* differ by more than *tol*.
fn differences(a: &Value, b: &Value, tol: f64, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() > tol {
                out.push(format!("{path}: {x} vs {y}"));
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                differences(p, q, tol, &format!("{path}[{i}]"), out);
            }
        }
        (Value::Object(x), Value::Object(y)) if x.len() == y.len() => {
            for (k, p) in x {
                match y.get(k) {
                    Some(q) => differences(p, q, tol, &format!("{path}.{k}"), out),
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
        } = framed_records::to_file(features::recognise(&working), &frame, &working);
        for plate in &features.plates {
            for end in [*plate.lo, *plate.hi] {
                assert!(
                    (end - (end * 1000.0).round() / 1000.0).abs() < 1e-10,
                    "{name}: {plate:?}"
                );
            }
        }
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
        compared += 1;
        let mut found = Vec::new();
        // 1e-7: less than any published grid's step (1e-6 mm at the finest), more than the
        // round-off a motion leaves in an unrounded value.
        differences(&expected, &got, 1e-7, &name, &mut found);
        problems.extend(found);
    }
    assert!(
        compared >= 9,
        "only {compared} golden parts have a full frame"
    );
    assert!(problems.is_empty(), "differ:\n{}", problems.join("\n"));
}

/// Record counts by family. Step levels and risers are body evidence read along the frame's z,
/// whose counts change under a tiny turn (golden_polygonal_boss: 5 risers unturned, 3 at 1e-8
/// rad). That is recognition's, not publication's: the frame and working part are inferred as
/// before #70. Not compared, as in [`a_moved_copy_publishes_the_carried_records`].
fn counts(r: &Recognition) -> Vec<(String, usize)> {
    records(r)
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| !["risers", "step_levels"].contains(&k.as_str()))
        .filter_map(|(k, v)| Some((k.clone(), v.as_array()?.len())))
        .collect()
}

/// A part turned from its file's axes by a tiny angle is recognised as the aligned part at
/// every angle up to and beyond those a publication snap could accept (#70: recognition stays
/// in the frame as inferred; the snap only publishes). Where the turn moves no point of the
/// part by more than the coordinate floor (here up to sm-hanger's 2.42e-9 rad and a little
/// more), the records come back in the file's axes as the unrotated part's: the same file-coordinate fields, and values to the floor (in any order ties among equal records leave).
#[test]
fn a_tiny_rotation_is_recognised_and_published_as_the_aligned_part() {
    let mut compared = 0;
    for path in golden() {
        let unmoved = correspondence::recognise(&read_step_file_placed(&path, &IDENTITY).unwrap());
        let RecordFrame::Inferred(f0) = &unmoved.frame else {
            continue;
        };
        let exact = |v: [f64; 3]| v.iter().all(|c| [0.0, 1.0, -1.0].contains(c));
        if ![f0.x, f0.y, f0.z].into_iter().all(exact) {
            continue; // not aligned with the file's axes: nothing to snap
        }
        for radians in [1e-10, 2.42e-9, 2.5e-9, 1e-8, 1e-7, 1e-6, 1e-5, 4e-5, 1e-4] {
            let tilted = motion([0.6, 0.8, 0.0], radians, [0.0; 3]);
            let moved = correspondence::recognise(&read_step_file_placed(&path, &tilted).unwrap());
            assert!(
                matches!(moved.frame, RecordFrame::Inferred(_)),
                "{path:?} at {radians}"
            );
            assert_eq!(counts(&unmoved), counts(&moved), "{path:?} at {radians}");
            // A free gauge (a roll about a turned part's axis) may take another representative.
            if radians <= 2.5e-9 && f0.gauge == quiddity::frames::FrameGauge::Full {
                let mut found = Vec::new();
                differences(
                    &canonical(&records(&unmoved)),
                    &canonical(&records(&moved)),
                    // The turn moves unrounded values by up to the coordinate floor (1e-6 mm).
                    2e-6,
                    &format!("{path:?} at {radians}"),
                    &mut found,
                );
                assert!(found.is_empty(), "differ:\n{}", found.join("\n"));
            }
        }
        compared += 1;
    }
    assert!(compared >= 10, "only {compared} golden parts have a frame");
}

/// sm-hanger's frame is aligned with the file's axes up to 2.4e-9 rad: its records are published
/// through that signed permutation, so its fillets name the file's axis their cylinder runs
/// along, and step levels (read along the frame's z by specification, here the
/// file's x) acquire explicit origins and height directions. The frame itself, and so recognition, is as inferred.
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
    let aligned = [[0.0, 0.0, 1.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]];
    let off = [frame.x, frame.y, frame.z]
        .iter()
        .zip(&aligned)
        .flat_map(|(a, b)| (0..3).map(move |i| (a[i] - b[i]).abs()))
        .fold(0.0, f64::max);
    assert!(off > 0.0 && off < 3e-9, "{frame:?}");
    // For #120: ten proved rounds join the four retained fillets; bend skins 73/76
    // lose their former false fillet claims. For #127, twelve proved R3 inside
    // rounds join those fourteen; all twenty-six publish on file axes.
    assert_eq!(r.features.fillets.len(), 26);
    let defining = &r.features.defining["fillets"];
    let mut faces: Vec<_> = defining.iter().flatten().copied().collect();
    faces.sort_unstable();
    assert_eq!(
        faces,
        [
            3, 8, 14, 18, 22, 24, 27, 28, 30, 31, 35, 39, 43, 46, 52, 53, 54, 57, 58, 59, 63, 75,
            85, 87, 91, 98
        ]
    );
    for (fillet, faces) in r.features.fillets.iter().zip(defining) {
        assert_eq!(
            fillet.side,
            if part.frame_points_outward(faces[0]) == Some(true) {
                "convex"
            } else {
                "concave"
            }
        );
        let Surface::Cylinder { frame: cyl, .. } = &part.faces[faces[0]].surface else {
            panic!("a fillet is a cylinder");
        };
        let w = (0..3)
            .max_by(|&a, &b| cyl.z[a].abs().total_cmp(&cyl.z[b].abs()))
            .unwrap();
        assert_eq!(
            fillet.axis,
            quiddity::FilletAxis::Principal(['x', 'y', 'z'][w]),
            "{fillet:?}"
        );
    }
}

/// For #84: plate ends share the file-coordinate 0.001 mm publication grid. The
/// fingerprint keeps the actual face separation, independent of the publication's origin.
#[test]
fn mfcadpp_1000_plate_bounds_publish_once_on_the_file_grid() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = quiddity::read_step_file(&dir.join("mfcadpp/1000.step")).unwrap();
    let found = correspondence::recognise(&part);
    let plate = &found.features.plates[0];
    assert_eq!(plate.axis, 'x');
    assert_eq!((*plate.lo, *plate.hi), (0.0, 4.219));
    assert_eq!(plate.thickness(), 4.219);
    let fingerprints = serde_json::to_value(&found.fingerprints).unwrap();
    let plate = fingerprints["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "plates/0")
        .unwrap();
    assert!((plate["sizes"]["thickness"].as_f64().unwrap() - 4.219087637691).abs() < 1e-10);
    let at = [
        [1.0, 0.0, 0.0, 0.00045],
        [0.0, 1.0, 0.0, -0.00045],
        [0.0, 0.0, 1.0, 0.00045],
    ];
    let carried = map_features(found.features.clone(), &Rigid::from_placement(&at));
    assert_eq!(
        (
            *carried.features.plates[0].lo,
            *carried.features.plates[0].hi
        ),
        (0.0, 4.220)
    );
    let moved = correspondence::recognise(&part.transformed(&at).unwrap());
    assert_eq!(
        (*moved.features.plates[0].lo, *moved.features.plates[0].hi),
        (0.0, 4.220)
    );
    let moved = serde_json::to_value(&moved.fingerprints).unwrap();
    let moved = moved["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "plates/0")
        .unwrap();
    assert!(
        (moved["sizes"]["thickness"].as_f64().unwrap()
            - plate["sizes"]["thickness"].as_f64().unwrap())
        .abs()
            < 1e-10
    );
}

/// For #85: the inner skin of the frame is at -7.7; small nearby faces at -7.4
/// belong to details in the opening and must not extend the plate's face band.
#[test]
fn whistle_frame_plate_ends_on_its_own_skin() {
    let part =
        quiddity::read_step_file(&common::fixtures().join("whistle_frame_reference.step.gz"))
            .unwrap();
    let found = correspondence::recognise(&part);
    let plate = &found.features.plates[0];
    assert_eq!(plate.axis, 'x');
    assert_eq!((*plate.lo, *plate.hi), (-14.7, -7.7));
    assert!((plate.thickness() - 7.0).abs() < 1e-12);
    assert_eq!(found.features.defining["plates"][0], [4, 6, 10, 14, 39, 62]);
    // The same skins and dimension survive reversal and an off-grid translation.
    let at = [
        [-1.0, 0.0, 0.0, 5.1234],
        [0.0, -1.0, 0.0, -9.8765],
        [0.0, 0.0, 1.0, 12.3456],
    ];
    let moved_part = part.transformed(&at).unwrap();
    for candidate in [&part, &moved_part] {
        let skin =
            features::plates::discover(&features::Context::new(candidate), &Default::default())
                .into_iter()
                .find(|o| {
                    let mut faces = o.defining.clone();
                    faces.sort_unstable();
                    faces == [4, 6, 10, 14, 39, 62]
                })
                .expect("same plate skins under axis reversal");
        assert!((skin.record.thickness() - 7.0).abs() < 1e-12);
    }
    let moved = correspondence::recognise(&moved_part);
    assert_eq!(
        moved.features.defining["plates"][0],
        found.features.defining["plates"][0]
    );
    assert!((moved.features.plates[0].thickness() - 7.0).abs() < 1e-12);
    let fingerprints = serde_json::to_value(&found.fingerprints).unwrap();
    let plate = fingerprints["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "plates/0")
        .unwrap();
    assert!((plate["sizes"]["thickness"].as_f64().unwrap() - 7.0).abs() < 1e-12);
}

/// The six physical thickness corrections agreed for #71 remain exact in the
/// fingerprints, even though the displayed positions now use the file's grid.
#[test]
fn plate_support_selection_preserves_the_six_plane_corrections() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    for (file, expected) in [
        ("gramel/GRM-03_thumbwheel_drive_screw.step", 2.5),
        ("mfcadpp/10049.step", 4.519223921896),
        ("mfcadpp_holdout/182.step", 1.128803214932),
        ("mfcadpp_holdout/340.step", 1.593091728916),
        ("mfcadpp_holdout/808.step", 3.0),
        ("ttt_inputs/sm-hanger.step", 4.0),
    ] {
        let part = quiddity::read_step_file(&dir.join(file)).unwrap();
        let found = correspondence::recognise(&part);
        let fingerprints = serde_json::to_value(&found.fingerprints).unwrap();
        let plate = fingerprints["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == "plates/0")
            .unwrap();
        assert!(
            (plate["sizes"]["thickness"].as_f64().unwrap() - expected).abs() < 1e-10,
            "{file}"
        );
    }
}
