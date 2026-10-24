//! Rust frame snapshots (axes 1e-9, origin 1e-9 of part size) and unchanged motion-equivariance references.

use super::common;

use std::path::PathBuf;

use quiddity::frames::{
    FrameGauge, FrameRefusal, FramedError, PartFrame, infer_part_frame, prepare_framed_step,
};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed, read_step_placed};
use serde_json::Value;

/// The rotation by *degrees* about *axis* (Rodrigues), then *translation*.
fn motion(axis: [f64; 3], degrees: f64, translation: [f64; 3]) -> Placement {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = axis.map(|c| c / n);
    let (s, c) = degrees.to_radians().sin_cos();
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

/// #91's mixed-direction parts choose file axes as read. Test their vote-only
/// equivariance between two generic poses, while separate tests pin file-axis transitions.
pub(super) fn prefers_file_axes(file: &str) -> bool {
    matches!(
        file,
        "cadgenbench_inputs/cgb217.step.gz"
            | "cadgenbench_inputs/cgb241.step"
            | "golden_oriented_slots.step"
    )
}

pub(super) fn vote_only_pose(at: &Placement) -> Placement {
    let anchor = motion([2., -3., 5.], 19., [0.; 3]);
    [0, 1, 2].map(|i| {
        [0, 1, 2, 3].map(|j| {
            (0..3).map(|k| at[i][k] * anchor[k][j]).sum::<f64>()
                + if j == 3 { at[i][3] } else { 0. }
        })
    })
}

pub(super) fn assert_no_file_axis_frame(part: &quiddity::Part) {
    use quiddity::kernel::geom::Surface;
    let axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let available = axes
        .iter()
        .filter(|axis| {
            part.faces.iter().any(|face| {
                let direction = match face.surface {
                    Surface::Plane { frame } | Surface::Cylinder { frame, .. } => frame.z,
                    _ => return false,
                };
                quiddity::kernel::geom::dot(**axis, direction).abs() >= 1. - 1e-10
            })
        })
        .count();
    assert!(available < 2, "generic pose gained file axes");
}

fn captured() -> Value {
    common::load("captured/frames.json")
}

fn captured_motion(captured: &Value) -> Placement {
    let m = &captured["motion"];
    let v = |k: &str| -> [f64; 3] {
        let a = m[k].as_array().unwrap();
        [0, 1, 2].map(|i| a[i].as_f64().unwrap())
    };
    motion(
        v("axis"),
        m["angle_degrees"].as_f64().unwrap(),
        v("translation"),
    )
}

fn vector(v: &Value) -> [f64; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap())
}

fn largest_difference(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max)
}

fn apply(m: &Placement, p: [f64; 3], point: bool) -> [f64; 3] {
    let t = |i: usize| if point { m[i][3] } else { 0.0 };
    [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + t(i))
}

/// How the frame of the part under *motion* differs from the unmoved frame carried by it: the
/// same refusal or gauge, the origin to 1e-9 of *size*, and for a full gauge the axes to 1e-9
/// (in the other gauges the axes are representatives, which may turn differently).
fn equivariance(
    motion: &Placement,
    unmoved: &Result<PartFrame, FrameRefusal>,
    moved: &Result<PartFrame, FrameRefusal>,
    size: f64,
) -> (Vec<&'static str>, f64, String) {
    match (unmoved, moved) {
        (Err(a), Err(b)) if a == b => (vec![], 0.0, String::new()),
        (Ok(a), Ok(b)) => {
            let (mut fields, mut text) = (Vec::new(), Vec::new());
            if a.gauge != b.gauge {
                fields.push("gauge");
                text.push(format!("gauge {:?} unmoved, {:?} moved", a.gauge, b.gauge));
            }
            if a.gauge == FrameGauge::Full {
                for (name, p, q) in [("x", a.x, b.x), ("y", a.y, b.y), ("z", a.z, b.z)] {
                    let off = largest_difference(apply(motion, p, false), q);
                    if off > 1e-9 {
                        fields.push(name);
                        text.push(format!("{name} off {off:.1e}"));
                    }
                }
            }
            let off = largest_difference(apply(motion, a.origin, true), b.origin);
            if off > 1e-9 * size.max(1.0) {
                fields.push("origin");
                text.push(format!("origin off {off:.1e}"));
            }
            (fields, off, text.join("; "))
        }
        _ => (
            vec!["refusal"],
            0.0,
            format!(
                "unmoved {:?}, moved {:?}",
                unmoved.as_ref().map(|f| f.gauge),
                moved.as_ref().map(|f| f.gauge)
            ),
        ),
    }
}

/// Also checks that the port's frame moves with the part: listed as case `equivariance`.
#[test]
fn rust_snapshot_frames_and_equivariance() {
    let captured = captured();
    let generic = captured_motion(&captured);
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
    let known = common::load("references/frame_equivariance.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("references/frame_equivariance.json", known);
    let parts: Vec<&Value> = captured["parts"].as_array().unwrap().iter().collect();
    let path = |entry: &Value| -> Option<PathBuf> {
        let file = entry["file"].as_str().unwrap();
        match entry["source"].as_str().unwrap() {
            "built" => Some(common::fixtures().join("captured/frames").join(file)),
            "fixture" => Some(common::fixtures().join(file)),
            _ => corpus.as_ref().map(|d| d.join(file)),
        }
    };
    let found = common::parallel::map(&parts, |entry| {
        let file = entry["file"].as_str().unwrap();
        let Some(path) = path(entry) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut frames = Vec::new();
        let mut snapshot = serde_json::Map::new();
        for (case, placement) in [("frame", IDENTITY), ("moved", generic)] {
            let part = read_step_file_placed(&path, &placement).unwrap();
            let size = part.bounds().diagonal();
            let frame = infer_part_frame(&part);
            let value = frame.as_ref().map_or_else(
                |r| serde_json::json!({"refused":r.as_str()}),
                |f| serde_json::to_value(f).unwrap(),
            );
            snapshot.insert(case.into(), value);
            frames.push((frame, size));
        }
        let equal = |a: &Value, b: &Value| {
            ["frame", "moved"].iter().enumerate().all(|(i, case)| {
                let (a, b) = (&a[case], &b[case]);
                if a.get("refused").is_some() || b.get("refused").is_some() {
                    return a == b;
                }
                a["gauge"] == b["gauge"]
                    && ["x", "y", "z"]
                        .iter()
                        .all(|k| largest_difference(vector(&a[k]), vector(&b[k])) <= 1e-9)
                    && largest_difference(vector(&a["origin"]), vector(&b["origin"]))
                        <= 1e-9 * frames[i].1.max(1.0)
            })
        };
        common::snapshot::check_with(
            &common::fixtures(),
            &format!("frames/{}/{}", entry["source"].as_str().unwrap(), file),
            &Value::Object(snapshot),
            &equal,
        );
        // The three #91 cases deliberately change query frame on leaving file axes.
        // Keep strict generic-to-generic equivariance instead of allowing field differences.
        let (fields, off, text) = if prefers_file_axes(file) {
            let p0 = read_step_file_placed(&path, &vote_only_pose(&IDENTITY)).unwrap();
            let p1 = read_step_file_placed(&path, &vote_only_pose(&generic)).unwrap();
            assert_no_file_axis_frame(&p0);
            assert_no_file_axis_frame(&p1);
            equivariance(
                &generic,
                &infer_part_frame(&p0),
                &infer_part_frame(&p1),
                frames[0].1,
            )
        } else {
            equivariance(&generic, &frames[0].0, &frames[1].0, frames[0].1)
        };
        if !fields.is_empty() {
            out.push((file.to_string(), "equivariance", fields, off, text));
        }
        out
    });
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for (file, case, fields, off, text) in found.into_iter().flatten() {
        let entry = || {
            let mut e = serde_json::json!({"file": file, "case": case, "fields": fields});
            if fields.contains(&"origin") {
                e["origin_within"] =
                    serde_json::json!(format!("{:.0e}", off * 1.5).parse::<f64>().unwrap());
            }
            e
        };
        match known
            .iter()
            .position(|k| k["file"] == file.as_str() && k["case"] == case)
        {
            Some(i) => {
                seen.push(i);
                let k = &known[i];
                let listed: Vec<&str> = k["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| f.as_str().unwrap())
                    .collect();
                let within = k["origin_within"].as_f64().unwrap_or(0.0);
                if listed != fields || (fields.contains(&"origin") && off > within) {
                    problems.push(format!("{file} {case} changed: {text}\n  now: {}", entry()));
                }
            }
            None => problems.push(format!("{file} {case}: {text}\n  needs: {}", entry())),
        }
    }
    for (i, k) in known.iter().enumerate() {
        let corpus_entry = parts
            .iter()
            .any(|p| p["file"] == k["file"] && p["source"] == "corpus");
        if !seen.contains(&i) && !(corpus_entry && corpus.is_none()) {
            problems.push(format!("{k} is listed but now agrees: remove it"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Python's `test_frame_point_transforms_are_inverse`.
#[test]
fn frame_point_transforms_are_inverse() {
    let frame = PartFrame {
        origin: [10.0, 20.0, 30.0],
        x: [0.0, 1.0, 0.0],
        y: [0.0, 0.0, 1.0],
        z: [1.0, 0.0, 0.0],
        gauge: FrameGauge::Full,
    };
    assert_eq!(frame.to_local([14.0, 25.0, 36.0]), [5.0, 6.0, 4.0]);
    assert_eq!(frame.to_world([5.0, 6.0, 4.0]), [14.0, 25.0, 36.0]);
}

/// The working part is the caller's part in the frame's local coordinates, face for face and
/// edge for edge (Python's `test_framed_working_shape_uses_the_published_local_coordinates`),
/// for one part of each gauge, as read and under the generic motion.
#[test]
fn working_part_is_the_caller_part_in_local_coordinates() {
    let generic = captured_motion(&captured());
    let dir = common::fixtures().join("captured/frames");
    for (file, gauge) in [
        ("asymmetric_bored_moved.step", FrameGauge::Full),
        ("box_moved.step", FrameGauge::Orthogonal),
        ("cylinder_rot_x37.step", FrameGauge::Axial),
    ] {
        let bytes = std::fs::read(dir.join(file)).unwrap();
        for placement in [IDENTITY, generic] {
            let caller = read_step_placed(&bytes, &placement).unwrap();
            let framed = prepare_framed_step(&bytes, &placement).unwrap();
            assert_eq!(framed.frame.gauge, gauge, "{file}");
            assert_eq!(framed.frame, infer_part_frame(&caller).unwrap(), "{file}");
            assert_eq!(framed.part.edges.len(), caller.edges.len(), "{file}");
            for (local, edge) in framed.part.edges.iter().zip(&caller.edges) {
                for (p, q) in [(local.start, edge.start), (local.end, edge.end)] {
                    let want = framed.frame.to_local(q);
                    assert!(
                        (0..3).all(|i| (p[i] - want[i]).abs() < 1e-8),
                        "{file}: {p:?} against {want:?}"
                    );
                }
            }
        }
    }
}

/// Framed recognition finds the bore of the full-gauge part, in local coordinates, wherever
/// the caller placed it; the sphere is refused for want of a direction, as Python refuses it.
#[test]
fn framed_recognition_follows_the_part() {
    let generic = captured_motion(&captured());
    let dir = common::fixtures().join("captured/frames");
    let bytes = std::fs::read(dir.join("asymmetric_bored_moved.step")).unwrap();
    let records: Vec<String> = [IDENTITY, generic]
        .iter()
        .map(|placement| {
            let framed = prepare_framed_step(&bytes, placement).unwrap().recognise();
            assert_eq!(framed.features.holes.len(), 1);
            let hole = serde_json::to_value(&framed.features.holes).unwrap();
            format!("{:?}", framed.features.defining["holes"]) + &hole["axis"].to_string()
        })
        .collect();
    assert_eq!(records[0], records[1]);
    let sphere = std::fs::read(dir.join("sphere.step")).unwrap();
    match prepare_framed_step(&sphere, &IDENTITY) {
        Err(FramedError::Frame(r)) => assert_eq!(r, FrameRefusal::NoAnalyticDirection),
        other => panic!("sphere: {:?}", other.map(|f| f.frame)),
    }
}

/// The default recognition reports the working part's faces under the caller's indices
/// (`correspondence::recognise`): the in-memory transform must preserve face numbering.
/// On every corpus part with a frame, each working face has the caller face's surface
/// kind and edges, and each of those edges' ends is the caller's carried into the frame (to 1e-6
/// of the part's size).
#[test]
fn working_faces_are_the_caller_faces_on_every_corpus_part() {
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
    let (mut checked, mut problems) = (0, Vec::new());
    let found = common::parallel::map(&files, |name| {
        let path = dir.join(name);
        let Ok(caller) = read_step_file_placed(&path, &IDENTITY) else {
            return None;
        };
        let Ok(framed) = quiddity::frames::prepare_framed_file(&path, &IDENTITY) else {
            return None;
        };
        let size = caller
            .edges
            .iter()
            .flat_map(|e| e.start.iter().chain(&e.end))
            .fold(1.0f64, |m, c| m.max(c.abs()));
        let mut out = Vec::new();
        for (i, (local, face)) in framed.part.faces.iter().zip(&caller.faces).enumerate() {
            let edges = caller.face_edges(i);
            if local.surface.kind() != face.surface.kind() || framed.part.face_edges(i) != edges {
                out.push(format!("{name}: face {i} is another face"));
                continue;
            }
            for e in edges {
                let (l, c) = (&framed.part.edges[e], &caller.edges[e]);
                for (p, q) in [(l.start, c.start), (l.end, c.end)] {
                    if largest_difference(p, framed.frame.to_local(q)) > 1e-6 * size {
                        out.push(format!("{name}: face {i} edge {e} at {p:?}, not {q:?}"));
                    }
                }
            }
        }
        Some(out)
    });
    for out in found.into_iter().flatten() {
        checked += 1;
        problems.extend(out);
    }
    assert!(checked > 90, "only {checked} corpus parts had a frame");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// #81: the AP242 minority angled faces must not choose roll, and a real placement must
/// carry the frame and the same fillet evidence. A 1° motion is deliberately inside the old
/// direction-class tolerance, so these copies must be transformed in memory, not re-exported.
#[test]
fn axis_majorities_and_intrinsic_frames_survive_sixty_and_one_degree_motions() {
    use quiddity::{correspondence, frames::RecordFrame};
    let corpus = common::corpus_dir();
    let mut cases = vec![
        (
            common::fixtures().join("nist_ctc_02_asme1_ap242-e2.stp.gz"),
            20,
            0,
            0,
        ),
        (
            common::fixtures().join("nist_ftc_07_asme1_ap242-e2.stp.gz"),
            16,
            4,
            5,
        ),
    ];
    if let Some(dir) = corpus {
        cases.extend([
            (dir.join("nist/nist_ctc_02_asme1_rc.stp"), 21, 0, 0),
            (dir.join("nist/nist_ftc_07_asme1_rd.stp"), 16, 4, 5),
        ]);
    }
    for (path, fillets, bosses, plates) in cases {
        let part = quiddity::read_step_file(&path).unwrap();
        let frame = infer_part_frame(&part).unwrap();
        assert!(
            [frame.x, frame.y, frame.z]
                .iter()
                .all(|a| a.iter().any(|c| c.abs() == 1.)),
            "{path:?}: {frame:?}"
        );
        let mut original_faces = None;
        for (pose, placement) in [
            IDENTITY,
            motion([0., 0., 1.], 60., [12.3, -45.6, 78.9]),
            motion([0., 0., 1.], 1., [12.3, -45.6, 78.9]),
            motion([1., 0., 0.], 60., [12.3, -45.6, 78.9]),
            motion([1., 0., 0.], 1., [12.3, -45.6, 78.9]),
        ]
        .into_iter()
        .enumerate()
        {
            let moved = part.transformed(&placement).unwrap();
            let recognised = correspondence::recognise(&moved);
            let RecordFrame::Inferred(actual) = recognised.frame else {
                panic!("{path:?}: frame refused")
            };
            let (fields, _, why) = equivariance(
                &placement,
                &Ok(frame),
                &Ok(actual),
                part.bounds().diagonal(),
            );
            let is_ftc_rd = path.file_name().unwrap() == "nist_ftc_07_asme1_rd.stp";
            let gains_file_axes = (is_ftc_rd
                || path.file_name().unwrap() == "nist_ftc_07_asme1_ap242-e2.stp.gz")
                && matches!(pose, 2 | 4);
            if gains_file_axes {
                // #91: the real 1-degree minor directions now establish file axes.
                // This changes the query; it is not a failure of generic equivariance.
                let expected = if pose == 2 {
                    vec!["y", "z"]
                } else {
                    vec!["x", "y", "z"]
                };
                assert_eq!(fields, expected, "{path:?}: {why}");
                assert!(
                    [actual.x, actual.y, actual.z]
                        .iter()
                        .all(|a| a.iter().any(|c| c.abs() > 1. - 1e-10))
                );
                let at = vote_only_pose(&placement);
                let generic = part.transformed(&at).unwrap();
                assert_no_file_axis_frame(&generic);
                let generic_found = correspondence::recognise(&generic);
                assert_eq!(generic_found.features.fillets.len(), fillets);
                assert_eq!(generic_found.features.polygonal_bosses.len(), bosses);
                assert_eq!(generic_found.features.plates.len(), plates);
                let mut generic_faces = generic_found.features.defining["fillets"].clone();
                generic_faces.sort();
                assert_eq!(Some(&generic_faces), original_faces.as_ref());
            } else {
                assert!(fields.is_empty(), "{path:?}: {why}");
            }
            assert_eq!(
                recognised.features.fillets.len(),
                if gains_file_axes && !is_ftc_rd {
                    12
                } else {
                    fillets
                },
                "{path:?}: {placement:?}"
            );
            assert_eq!(
                recognised.features.polygonal_bosses.len(),
                if gains_file_axes { 0 } else { bosses },
                "{path:?}"
            );
            assert_eq!(recognised.features.plates.len(), plates, "{path:?}");
            let mut faces = recognised.features.defining["fillets"].clone();
            faces.sort();
            if let Some(original) = &original_faces {
                let mut expected = original.clone();
                if gains_file_axes && !is_ftc_rd {
                    let missing = if pose == 2 {
                        [224, 161, 170, 158]
                    } else {
                        [160, 161, 191, 158]
                    };
                    for face in missing {
                        assert!(expected.contains(&vec![face]));
                        expected.retain(|f| *f != vec![face]);
                    }
                }
                assert_eq!(faces, expected, "{path:?}");
            } else {
                original_faces = Some(faces);
            }
        }
        if path.file_name().unwrap() == "nist_ctc_02_asme1_ap242-e2.stp.gz" {
            let faces = original_faces.unwrap();
            for face in [
                391, 394, 396, 399, 475, 519, 522, 532, 542, 545, 548, 625, 626, 630, 631, 77, 78,
                79,
            ] {
                assert!(faces.contains(&vec![face]), "missing AP242 fillet {face}");
            }
            // rc's 21st fillet is its face 635, R1.092560827904 on a 2° cylinder.
            // That small feature is absent from this AP242 geometry, whose minimum cylinder
            // radius is 4.19 mm. Do not invent a 21st record just to match another edition.
            assert!(
                part.faces
                    .iter()
                    .filter_map(|f| match f.surface {
                        quiddity::kernel::geom::Surface::Cylinder { radius, .. } => Some(radius),
                        _ => None,
                    })
                    .all(|r| r > 4.18)
            );
        }
    }
}

#[test]
fn majority_frame_recovers_specify_fillets_and_parallel_support_chamfer() {
    use quiddity::{correspondence, frames::RecordFrame};
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    for (file, family, count, faces) in [
        (
            "cadgenbench_inputs/cgb202.step.gz",
            "fillets",
            279,
            vec![
                1248, 1250, 1453, 1464, 1473, 1475, 1477, 1623, 1631, 2056, 2057,
            ],
        ),
        ("cadgenbench_inputs/cgb207.step", "fillets", 20, vec![176]),
        ("mfcadpp/10252.step", "oriented_chamfers", 1, vec![2]),
    ] {
        let part = quiddity::read_step_file(&dir.join(file)).unwrap();
        let recognised = correspondence::recognise(&part);
        let RecordFrame::Inferred(frame) = recognised.frame else {
            panic!("{file}: frame refused")
        };
        assert!(
            [frame.x, frame.y, frame.z]
                .iter()
                .all(|a| a.iter().any(|c| c.abs() == 1.)),
            "{file}"
        );
        let found = &recognised.features.defining[family];
        assert_eq!(found.len(), count, "{file}/{family}");
        for face in faces {
            assert!(
                found.contains(&vec![face]),
                "{file}: missing {family}:{face}"
            );
        }
        if file == "cadgenbench_inputs/cgb202.step.gz" {
            assert_eq!(recognised.features.blends.len(), 62);
            // #89: the old 296 included 17 concave cylinders, including 13 of the
            // #81 "recovered" faces. Pin their actual material side instead of a false
            // convex label; the eleven genuinely convex recovered faces stay above.
            for face in [
                29, 31, 46, 571, 657, 662, 806, 876, 890, 984, 998, 1069, 1087, 1092, 1104, 1225,
                1346,
            ] {
                assert_eq!(part.frame_points_outward(face), Some(false), "face {face}");
                assert!(!found.contains(&vec![face]), "concave fillet {face}");
            }
        }
        if family == "oriented_chamfers" {
            let chamfer = &recognised.features.oriented_chamfers[0];
            assert_eq!(
                (chamfer.length, chamfer.leg1, chamfer.leg2, chamfer.angle),
                (18.694, 0.54, 0.54, 45.)
            );
            assert_eq!(*chamfer.corner, [31.254, 12.205, 0.]);
        }
    }
}

#[test]
fn genuinely_rotated_corpus_direction_sets_keep_their_winning_frames() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    for (file, expected_y) in [
        (
            "cadgenbench_inputs/cgb203.step",
            [0.18196311701898504, 0.9833053564609191, 0.],
        ),
        // Neither part has two file-axis classes; the #81 vote is unchanged.
        (
            "cadgenbench/threaded_connector_109.step",
            [-0.45979133716192094, -0.8880269851028474, 0.],
        ),
    ] {
        let part = quiddity::read_step_file(&dir.join(file)).unwrap();
        let frame = infer_part_frame(&part).unwrap();
        assert!(
            largest_difference(frame.y, expected_y) < 1e-10,
            "{file}: {frame:?}"
        );
        let placement = motion([0., 0., 1.], 1., [12.3, -45.6, 78.9]);
        let moved = part.transformed(&placement).unwrap();
        let (fields, _, why) = equivariance(
            &placement,
            &Ok(frame),
            &infer_part_frame(&moved),
            part.bounds().diagonal(),
        );
        assert!(fields.is_empty(), "{file}: {why}");
    }
}

/// Equal complete direction sets must choose the file axes. Each box has six distinct planes; their four Z planes are shared
/// direction support, so both competing bases have exactly eight distinct surfaces.
#[test]
fn exact_surface_support_tie_prefers_file_axes() {
    use quiddity::kernel::{brep::Part, geom::Surface};
    let source =
        quiddity::read_step_file(&common::fixtures().join("captured/frames/box_rot_x30.step"))
            .unwrap();
    assert_eq!(source.faces.len(), 6);
    assert!(
        source
            .faces
            .iter()
            .all(|f| matches!(f.surface, Surface::Plane { .. }))
    );
    let first = source
        .transformed(&motion([1., 0., 0.], -30., [0.; 3]))
        .unwrap();
    let second = first
        .transformed(&motion([0., 0., 1.], 30., [100., 100., 100.]))
        .unwrap();
    let mut faces = first.faces.clone();
    let mut edges = first.edges.clone();
    let mut solids = first.solids.clone();
    let vertex_offset = edges
        .iter()
        .map(|e| e.vertices.0.max(e.vertices.1))
        .max()
        .unwrap()
        + 1;
    for mut face in second.faces.clone() {
        face.solid = face.solid.map(|i| i + solids.len());
        for wire in &mut face.loops {
            for (edge, _) in &mut wire.edges {
                *edge += edges.len();
            }
        }
        for (edge, _) in &mut face.pcurves {
            *edge += edges.len();
        }
        faces.push(face);
    }
    for mut solid in second.solids.clone() {
        for face in &mut solid.faces {
            *face += first.faces.len();
        }
        solids.push(solid);
    }
    for mut edge in second.edges.clone() {
        edge.vertices.0 += vertex_offset;
        edge.vertices.1 += vertex_offset;
        edges.push(edge);
    }
    let part = Part::new(faces, edges, solids);
    let frame = infer_part_frame(&part).unwrap();
    assert!(
        [frame.x, frame.y, frame.z]
            .iter()
            .all(|a| a.iter().any(|c| c.abs() == 1.)),
        "{frame:?}"
    );
}

/// The oblique channel's coplanar patches used to outvote the file axes 17:16 by face.
/// Its complete basis has only 10 distinct supports against 15 on the file axes. Pin
/// the real slot, its recess, the void and the angled step that the split-face vote lost.
#[test]
fn split_channel_surfaces_do_not_outvote_file_axes() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let part = quiddity::read_step_file(&dir.join("mfcadpp_regressions/11512.step")).unwrap();
    let frame = infer_part_frame(&part).unwrap();
    assert!(
        [frame.x, frame.y, frame.z]
            .iter()
            .all(|a| a.iter().any(|c| c.abs() == 1.)),
        "{frame:?}"
    );
    let recognised = quiddity::correspondence::recognise(&part);
    assert_eq!(recognised.features.plates.len(), 1);
    assert_eq!(recognised.features.risers.len(), 2);
    assert_eq!(recognised.features.chamfers.len(), 1);
    assert!(recognised.features.oriented_chamfers.is_empty());
    assert!(recognised.features.fillets.is_empty());
    // The former fillet was an inward-facing (concave) R1 cylinder, not an external round.
    assert_eq!(part.frame_points_outward(28), Some(false));
    for (family, faces) in [
        ("rectangular_blind_slots", vec![18, 19, 20, 29]),
        ("section_recesses", vec![18, 19, 20, 29]),
        ("interior_voids", vec![13, 14]),
        ("angled_steps", vec![32]),
    ] {
        assert!(
            recognised.features.defining[family].contains(&faces),
            "{family}: {faces:?}"
        );
    }
}

/// #91: the file-axis candidate wins even when the distinct-surface vote is larger
/// on the angled features. Signs/order remain the existing material representatives.
#[test]
fn available_file_axes_override_the_surface_vote() {
    let mut cases = vec![(
        common::fixtures().join("golden_oriented_slots.step"),
        [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
    )];
    if let Some(dir) = common::corpus_dir() {
        cases.extend([
            (
                dir.join("cadgenbench_inputs/cgb217.step.gz"),
                [[0., 0., 1.], [1., 0., 0.], [0., 1., 0.]],
            ),
            (
                dir.join("cadgenbench_inputs/cgb241.step"),
                [[0., 0., 1.], [-1., 0., 0.], [0., -1., 0.]],
            ),
        ]);
    }
    for (path, expected) in cases {
        let part = quiddity::read_step_file(&path).unwrap();
        let frame = infer_part_frame(&part).unwrap();
        assert_eq!([frame.x, frame.y, frame.z], expected, "{path:?}");
        let generic = motion([1., 2., 3.], 37., [12.3, -45.6, 78.9]);
        let first = part.transformed(&vote_only_pose(&IDENTITY)).unwrap();
        let second = part.transformed(&vote_only_pose(&generic)).unwrap();
        assert_no_file_axis_frame(&first);
        assert_no_file_axis_frame(&second);
        let (fields, _, why) = equivariance(
            &generic,
            &infer_part_frame(&first),
            &infer_part_frame(&second),
            part.bounds().diagonal(),
        );
        assert!(fields.is_empty(), "{path:?}: {why}");
    }
}

/// ADR 0011: bringing angled slots onto file axes changes their published family;
/// leaving both candidate bases oblique restores the unchanged surface-vote winner.
#[test]
fn oriented_slots_publish_in_file_axes_and_change_family_when_slots_align() {
    use quiddity::{correspondence, recognition};
    let part =
        quiddity::read_step_file(&common::fixtures().join("golden_oriented_slots.step")).unwrap();
    let found = correspondence::recognise(&part);
    assert_eq!(found.features.oriented_slots.len(), 3);
    assert_eq!(found.features.oriented_slot_patterns.len(), 1);
    assert!(found.features.slots.is_empty());
    let document = recognition::document(&found, part.faces.len());
    let slots: Vec<_> = document
        .records
        .iter()
        .filter(|r| r.family == "oriented_slots")
        .collect();
    assert_eq!(slots.len(), 3);
    assert!(slots.iter().all(|r| r.local.is_empty()));
    let oblique = part
        .transformed(&motion([0., 0., 1.], 13., [0.; 3]))
        .unwrap();
    assert_no_file_axis_frame(&oblique);
    let old_frame = infer_part_frame(&oblique).unwrap();
    let old = correspondence::recognise(&oblique);
    assert_eq!(old.features.slots.len(), 3);
    let back = motion([0., 0., 1.], -13., [0.; 3]);
    let returned = oblique.transformed(&back).unwrap();
    let returned_frame = infer_part_frame(&returned).unwrap();
    assert_eq!(
        [returned_frame.x, returned_frame.y, returned_frame.z],
        [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]]
    );
    assert!(largest_difference(apply(&back, old_frame.y, false), returned_frame.y) > 0.1);
    let returned = correspondence::recognise(&returned);
    assert_eq!(returned.features.oriented_slots.len(), 3);
    assert!(returned.features.slots.is_empty());
    let aligned = part
        .transformed(&motion([0., 0., 1.], -30., [0.; 3]))
        .unwrap();
    let found = correspondence::recognise(&aligned);
    let frame = infer_part_frame(&aligned).unwrap();
    assert!(
        [frame.x, frame.y, frame.z]
            .iter()
            .all(|a| a.iter().any(|c| c.abs() == 1.))
    );
    assert_eq!(found.features.slots.len(), 3);
    assert_eq!(found.features.slot_patterns.len(), 1);
    assert!(found.features.oriented_slots.is_empty());
    let document = recognition::document(&found, aligned.faces.len());
    assert!(
        document
            .records
            .iter()
            .filter(|r| r.family == "slots")
            .all(|r| r.local.is_empty())
    );
}

/// Native anatomy explains the #91 count falls: slot end walls are vertical through
/// the plate, and the removed risers are exterior stock walls. No slot wall disappears.
#[test]
fn file_axis_slotted_plates_keep_all_walls_without_rotated_levels() {
    use quiddity::{
        correspondence,
        kernel::geom::{self, Surface},
    };
    for (file, groups, side_faces) in [
        (
            "golden_oriented_slots.step",
            vec![vec![6, 7, 8, 9], vec![10, 11, 12, 13], vec![14, 15, 16, 17]],
            vec![6, 9, 10, 13, 14, 16],
        ),
        (
            "captured/3f89fd15d4993366.step.gz",
            vec![vec![6, 7, 8, 9], vec![16, 17, 18, 19]],
            vec![6, 8, 16, 18],
        ),
        (
            "captured/83ba2c37c00b754f.step.gz",
            vec![vec![6, 7, 8, 9], vec![16, 17, 18, 19]],
            vec![6, 8, 16, 18],
        ),
        (
            "captured/eaea9b3e567d92b4.step.gz",
            vec![vec![6, 7, 8, 9], vec![16, 17, 18, 19]],
            vec![6, 8, 16, 18],
        ),
        (
            "captured/ec6a3e238160bbb9.step.gz",
            vec![vec![6, 7, 8, 9], vec![16, 17, 18, 19]],
            vec![6, 8, 16, 18],
        ),
    ] {
        let part = quiddity::read_step_file(&common::fixtures().join(file)).unwrap();
        let found = correspondence::recognise(&part);
        assert!(found.features.step_levels.is_empty(), "{file}");
        assert!(found.features.slots.is_empty(), "{file}");
        let mut actual = found.features.defining["oriented_slots"].clone();
        actual.sort();
        assert_eq!(actual, groups, "{file}");
        let mut risers: Vec<_> = found.features.defining["risers"]
            .iter()
            .flatten()
            .copied()
            .collect();
        risers.sort();
        assert_eq!(risers, side_faces, "{file}");
        let frame = infer_part_frame(&part).unwrap();
        assert_eq!(
            [frame.x, frame.y, frame.z],
            [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
            "{file}"
        );
        for face in groups.iter().flatten() {
            let Surface::Plane { frame: wall } = part.faces[*face].surface else {
                panic!("{face}")
            };
            // Every slot wall runs through file Z, rather than being a horizontal floor.
            assert_eq!(wall.z[2], 0.);
            assert!(geom::dot(wall.z, frame.z).abs() < 0.999);
            let bounds = part.face_bounds(*face);
            assert!((bounds.max[2] - bounds.min[2] - 10.).abs() < 1e-6);
        }
        // Four stock walls per plate are axis-aligned exterior supports; the old
        // slot-based frame called these risers. They are not internal machining steps.
        for base in (0..part.faces.len()).step_by(if groups.len() == 3 { 18 } else { 10 }) {
            for face in [base, base + 1, base + 3, base + 5] {
                let Surface::Plane { frame: wall } = part.faces[face].surface else {
                    panic!("{face}")
                };
                assert!(wall.z[0].abs() == 1. || wall.z[1].abs() == 1.);
                assert!(!risers.contains(&face));
            }
        }
    }
}
