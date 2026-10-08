//! Face areas against OpenCascade's: every corpus face's [`Part::face_mass`] area against
//! `BRepGProp::SurfaceProperties` (`tools/capture_face_areas.py`), and against itself with the
//! part moved by a translation and a generic rotation.
//!
//! The port integrates over the region its 3D edge curves bound; OpenCascade over the region
//! the face's pcurves bound, with a fixed Gauss rule unless asked for a precision. So an area
//! that misses `face.area` (what Python reads) still agrees with OpenCascade when it matches
//! the adaptive rule (the fixed one is coarse there), or the adaptive area of the face with its
//! pcurves projected again from the edges (the file's or the import's pcurves stray from the
//! edges, within the edges' tolerance). Both are reported as counts. Any other difference, and
//! any change under motion, must be listed in `tests/fixtures/known_face_areas.json` with a
//! verdict, pinned to the area the port gives.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use haecceity::Part;
use haecceity::step::{IDENTITY, Placement, read_step_file_placed};
use serde_json::Value;

/// Agreement: relative, with a floor for faces too small for a relative test to mean anything.
const RELATIVE: f64 = 1e-6;
const FLOOR: f64 = 1e-9;
/// A placement must leave every area unchanged to the integration's resolution: each boundary
/// panel is resolved to 1e-9 of its own terms, and where a foot point grazes a B-spline
/// surface's side the corner it turns is located only as well as the inversion settles, so
/// moved areas agree to about 1e-8 (the largest corpus spread is 2e-8); a placement-dependent
/// boundary walk shows at 1e-5 and beyond.
const PLACED: f64 = 1e-7;
/// A listed difference's pinned area has changed beyond the same resolution.
const PINNED: f64 = 1e-7;

/// A non-round translation and 37° about a direction off every axis and diagonal.
fn motion() -> Placement {
    let t = [123.456, -78.9, 41.3];
    let n = (1.0f64 + 4.0 + 9.0).sqrt();
    let (x, y, z) = (1.0 / n, 2.0 / n, 3.0 / n);
    let (s, c) = 37f64.to_radians().sin_cos();
    let k = 1.0 - c;
    [
        [c + x * x * k, x * y * k - z * s, x * z * k + y * s, t[0]],
        [y * x * k + z * s, c + y * y * k, y * z * k - x * s, t[1]],
        [z * x * k - y * s, z * y * k + x * s, c + z * z * k, t[2]],
    ]
}

fn captured() -> Value {
    let path = common::fixtures().join("face_areas.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// What one file's faces gave: differences by (file, face, check) with the port's area and a
/// report, and how many faces agreed only with the adaptive rule or with rebuilt pcurves.
#[derive(Default)]
struct Outcome {
    found: BTreeMap<(String, u64, String), (Option<f64>, String)>,
    coarse: usize,
    pcurves: usize,
    negative: usize,
}

/// Every face of one file: against OpenCascade, and moved against unmoved.
fn check_file(dir: &Path, entry: &Value, out: &mut Outcome) {
    let file = entry["file"].as_str().unwrap();
    let path = dir.join(file);
    let part = read_step_file_placed(&path, &IDENTITY).unwrap();
    let moved = read_step_file_placed(&path, &motion()).unwrap();
    let column = |name: &str| -> Vec<Option<f64>> {
        entry[name]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_f64)
            .collect()
    };
    let (occ, fine, rebuilt) = (column("area"), column("area_fine"), column("area_3d"));
    let types = entry["types"].as_array().unwrap();
    assert_eq!(part.faces.len(), occ.len(), "{file}: face count");
    let area = |p: &Part, f: usize| p.face_mass(f).map(|m| m[0]);
    for f in 0..occ.len() {
        let got = area(&part, f);
        let key = |check: &str| (file.to_string(), f as u64, check.to_string());
        let near = |want: Option<f64>| matches!((got, want), (Some(g), Some(w)) if (g - w).abs() <= RELATIVE * w.abs() + FLOOR);
        if got.is_some_and(|g| g < 0.0) {
            out.negative += 1;
        }
        if near(occ[f]) {
        } else if near(fine[f]) {
            out.coarse += 1;
        } else if near(rebuilt[f]) {
            out.pcurves += 1;
        } else {
            let report = format!(
                "{} rust {got:?} occ {:?} (adaptive {:?}, pcurves rebuilt {:?})",
                types[f], occ[f], fine[f], rebuilt[f]
            );
            out.found.insert(key("occ"), (got, report));
        }
        let placed = area(&moved, f);
        let same = match (got, placed) {
            (Some(a), Some(b)) => (a - b).abs() <= PLACED * a.abs().max(b.abs()) + FLOOR,
            (None, None) => true,
            _ => false,
        };
        if !same {
            out.found.insert(
                key("placement"),
                (got, format!("{} rust {got:?} moved {placed:?}", types[f])),
            );
        }
    }
}

#[test]
fn face_areas_match_opencascade_and_ignore_placement() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let captured = captured();
    let entries = captured["files"].as_array().unwrap();
    let known = common::load("known_face_areas.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_face_areas.json", known);
    // The parts are independent; read and integrate them across the machine's cores.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let outcomes: Vec<Outcome> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|w| {
                let dir = &dir;
                scope.spawn(move || {
                    let mut out = Outcome::default();
                    for entry in entries.iter().skip(w).step_by(threads) {
                        check_file(dir, entry, &mut out);
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut found = BTreeMap::new();
    let (mut coarse, mut pcurves, mut negative) = (0, 0, 0);
    for out in outcomes {
        found.extend(out.found);
        (coarse, pcurves, negative) = (
            coarse + out.coarse,
            pcurves + out.pcurves,
            negative + out.negative,
        );
    }
    eprintln!(
        "{coarse} faces agree with OpenCascade's adaptive rule only, {pcurves} only once their \
         pcurves are projected from the edges; {} differences",
        found.len()
    );
    let mut problems = Vec::new();
    if negative > 0 {
        problems.push(format!("{negative} faces have a negative area"));
    }
    let mut listed = Vec::new();
    for k in known {
        let key = (
            k["file"].as_str().unwrap().to_string(),
            k["face"].as_u64().unwrap(),
            k["check"].as_str().unwrap().to_string(),
        );
        let pinned = k["rust"].as_f64();
        match found.get(&key) {
            None => problems.push(format!("{key:?} is listed but now agrees: remove it")),
            Some((got, report)) => {
                let same = match (got, pinned) {
                    (Some(g), Some(p)) => (g - p).abs() <= PINNED * p.abs() + FLOOR,
                    (None, None) => true,
                    _ => false,
                };
                if !same {
                    problems.push(format!(
                        "{key:?} is listed at {pinned:?} but changed: {report}"
                    ));
                }
            }
        }
        listed.push(key);
    }
    for (key, (_, report)) in &found {
        if !listed.contains(key) {
            problems.push(format!("{} face {} {}: {report}", key.0, key.1, key.2));
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
