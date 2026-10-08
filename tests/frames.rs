//! Framed recognition (`quiddity.frames`) against Python's frames: for every STEP fixture, every
//! corpus file and the hand-built parts of Python's frame tests, as read and under the generic
//! motion (37° about (1, 2, 3), then a translation), the frame `infer_part_frame` derives must be
//! Python's (`tests/fixtures/captured/frames.json`, `tools/capture_frames.py`): the same refusal,
//! or the same gauge with axes agreeing to 1e-9 and the origin to 1e-9 of the part's size. The
//! port's frame must also move with the part (case `equivariance`): the same gauge, the origin
//! carried to 1e-9 of the part's size and, for a full gauge, the axes to 1e-9.
//!
//! Differences are listed in `tests/fixtures/captured/known_frames.json`, each with its file,
//! `case` (`frame`: as read, `moved`, or `equivariance`), the `fields` that differ (`refusal`,
//! `gauge`, `x`, `y`, `z`, `origin`), for an origin the largest coordinate difference it may
//! reach (`origin_within`), and a verdict. A failing run prints the entry each difference needs.

mod common;

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

fn gauge_name(g: FrameGauge) -> &'static str {
    match g {
        FrameGauge::Full => "full",
        FrameGauge::Orthogonal => "orthogonal",
        FrameGauge::Axial => "axial",
    }
}

fn vector(v: &Value) -> [f64; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap())
}

fn largest_difference(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max)
}

/// How the port's frame differs from Python's: the fields that differ (`refusal`, `gauge`, `x`,
/// `y`, `z`, `origin`), the origin's largest coordinate difference, and a description. Axes
/// agree to 1e-9; the origin to 1e-9 of *size* (at least 1e-9).
fn frame_difference(
    got: &Result<PartFrame, FrameRefusal>,
    want: &Value,
    size: f64,
) -> (Vec<&'static str>, f64, String) {
    match (got, want["refused"].as_str()) {
        (Err(r), Some(w)) if r.as_str() == w => (vec![], 0.0, String::new()),
        (Err(r), _) => (
            vec!["refusal"],
            0.0,
            format!("refused {}, Python {}", r.as_str(), want),
        ),
        (Ok(f), Some(w)) => (
            vec!["refusal"],
            0.0,
            format!("{} frame, Python refused {w}", gauge_name(f.gauge)),
        ),
        (Ok(f), None) => {
            let (mut fields, mut text) = (Vec::new(), Vec::new());
            if gauge_name(f.gauge) != want["gauge"].as_str().unwrap() {
                fields.push("gauge");
                text.push(format!(
                    "gauge {}, Python {}",
                    gauge_name(f.gauge),
                    want["gauge"]
                ));
            }
            for (name, axis) in [("x", f.x), ("y", f.y), ("z", f.z)] {
                let w = vector(&want[name]);
                let off = largest_difference(axis, w);
                if off > 1e-9 {
                    fields.push(name);
                    text.push(format!("{name} {axis:?}, Python {w:?} (off {off:.1e})"));
                }
            }
            let w = vector(&want["origin"]);
            let off = largest_difference(f.origin, w);
            if off > 1e-9 * size.max(1.0) {
                fields.push("origin");
                text.push(format!(
                    "origin {:?}, Python {w:?} (off {off:.1e})",
                    f.origin
                ));
            }
            (fields, off, text.join("; "))
        }
    }
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
fn frames_agree_with_python() {
    let captured = captured();
    let generic = captured_motion(&captured);
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
    let known = common::load("captured/known_frames.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_frames.json", known);
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
        for (case, placement) in [("frame", IDENTITY), ("moved", generic)] {
            let part = read_step_file_placed(&path, &placement).unwrap();
            let size = part.bounds().diagonal();
            let frame = infer_part_frame(&part);
            let (fields, off, text) = frame_difference(&frame, &entry[case], size);
            if !fields.is_empty() {
                out.push((file.to_string(), case, fields, off, text));
            }
            frames.push((frame, size));
        }
        let (fields, off, text) = equivariance(&generic, &frames[0].0, &frames[1].0, frames[0].1);
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
