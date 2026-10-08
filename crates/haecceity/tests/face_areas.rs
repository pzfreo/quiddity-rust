//! Face areas against OpenCascade's: every corpus face's [`Part::face_mass`] area against
//! `face.area`, what Python's recognisers read (`BRepGProp::SurfaceProperties` with its fixed
//! Gauss rule, captured by `tools/capture_face_areas.py`), and against itself with the part
//! moved by a translation and a generic rotation.
//!
//! The port integrates over the region its 3D edge curves bound; OpenCascade over the region
//! the face's pcurves bound. Every difference, and every change under motion, must be listed in
//! `tests/fixtures/known_face_areas.json` with a verdict, pinned to the area the port gives
//! (`tools/known_face_areas.py` writes the list from the differences, the capture's further
//! OpenCascade references and the independent integrations of `tools/face_area_evidence.py`).
//! A file whose faces the reader does not walk in OpenCascade's order (a known inventory
//! divergence) is compared as one `order` entry instead of face by face.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use haecceity::Part;
use haecceity::geom::SurfaceType;
use haecceity::step::{IDENTITY, Placement, read_step_file_placed};
use serde_json::Value;

/// Agreement: relative, with a floor for faces too small for a relative test to mean anything.
const RELATIVE: f64 = 1e-6;
const FLOOR: f64 = 1e-9;
/// A placement must leave every area unchanged: each boundary panel is resolved to 1e-9 of its
/// own terms, and all but 16 corpus faces agree to 1e-12 moved. Four B-spline faces change by
/// 1.6e-9 to 2.2e-8 and are listed; tightening the panels a hundredfold leaves them, so the
/// cause is in the walk, not the quadrature (not traced). Whether this threshold holds across
/// platforms is open (see the list's entries).
const PLACED: f64 = 1e-9;
/// A listed difference's pinned area has changed.
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

/// A difference: the port's area, moved (for a placement difference), and a report.
type Found = (Option<f64>, Option<f64>, String);

/// The first face whose kind is not the one OpenCascade's traversal has there: faces the reader
/// does not walk in OpenCascade's order (tests/corpus.rs checks the order fully, with its
/// known divergences; kinds suffice to keep a reordered file's areas from being paired here).
fn misaligned(part: &Part, inventory: &[Value]) -> Option<String> {
    for (i, want) in inventory.iter().enumerate() {
        let got = match part.faces[i].surface.kind() {
            SurfaceType::Plane => "PLANE",
            SurfaceType::Cylinder => "CYLINDER",
            SurfaceType::Cone => "CONE",
            SurfaceType::Sphere => "SPHERE",
            SurfaceType::Torus => "TORUS",
            SurfaceType::Freeform | SurfaceType::Other => "OTHER",
        };
        let kind = want["type"].as_str().unwrap();
        let analytic = ["PLANE", "CYLINDER", "CONE", "SPHERE", "TORUS"].contains(&kind);
        if got != kind && (got != "OTHER" || analytic) {
            return Some(format!("face {i} is {got}, OpenCascade's is {kind}"));
        }
    }
    None
}

/// What one file's faces gave: differences by (file, face, check) with the port's area and a
/// report, and how many faces had a negative area.
#[derive(Default)]
struct Outcome {
    found: BTreeMap<(String, u64, String), Found>,
    negative: usize,
}

/// The relative change an area makes under motion, when both are there.
fn change(a: Option<f64>, b: Option<f64>) -> String {
    match (a, b) {
        (Some(a), Some(b)) => format!(
            " (relative change {:.1e})",
            (a - b).abs() / a.abs().max(b.abs())
        ),
        _ => String::new(),
    }
}

/// Every face of one file: against OpenCascade's `face.area`, and moved against unmoved.
fn check_file(dir: &Path, entry: &Value, inventory: &[Value], out: &mut Outcome) {
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
    let order = misaligned(&part, inventory);
    if let Some(report) = &order {
        let areas: Vec<_> = (0..part.faces.len()).map(|f| area(&part, f)).collect();
        let key = (file.to_string(), 0, "order".to_string());
        out.found
            .insert(key, (None, None, format!("{report}; areas {areas:?}")));
    }
    for f in 0..occ.len() {
        let got = area(&part, f);
        let key = |check: &str| (file.to_string(), f as u64, check.to_string());
        if got.is_some_and(|g| g < 0.0) {
            out.negative += 1;
        }
        let near = matches!((got, occ[f]), (Some(g), Some(w)) if (g - w).abs() <= RELATIVE * w.abs() + FLOOR);
        if !near && order.is_none() {
            let report = format!(
                "{} rust {got:?} occ {:?} (adaptive {:?}, pcurves rebuilt {:?})",
                types[f], occ[f], fine[f], rebuilt[f]
            );
            out.found.insert(key("occ"), (got, None, report));
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
                (
                    got,
                    placed,
                    format!(
                        "{} rust {got:?} moved {placed:?}{}",
                        types[f],
                        change(got, placed)
                    ),
                ),
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
    let corpus = common::load("corpus.json");
    let inventories: BTreeMap<&str, &[Value]> = corpus["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["file"].as_str().unwrap(),
                e["inventory"].as_array().unwrap().as_slice(),
            )
        })
        .collect();
    let known = common::load("known_face_areas.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_face_areas.json", known);
    // The parts are independent; read and integrate them across the machine's cores.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let outcomes: Vec<Outcome> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|w| {
                let (dir, inventories) = (&dir, &inventories);
                scope.spawn(move || {
                    let mut out = Outcome::default();
                    for entry in entries.iter().skip(w).step_by(threads) {
                        let inventory = inventories[entry["file"].as_str().unwrap()];
                        check_file(dir, entry, inventory, &mut out);
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut found = BTreeMap::new();
    let mut negative = 0;
    for out in outcomes {
        found.extend(out.found);
        negative += out.negative;
    }
    eprintln!("{} differences", found.len());
    // For tools/known_face_areas.py, which writes the list from them and the evidence.
    if let Some(dump) = std::env::var_os("FACE_AREAS_DUMP") {
        let rows: Vec<Value> = found
            .iter()
            .map(|((file, face, check), (rust, moved, report))| {
                serde_json::json!({"file": file, "face": face, "check": check, "rust": rust, "moved": moved, "report": report})
            })
            .collect();
        std::fs::write(dump, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    }
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
        let pinned = (k["rust"].as_f64(), k["moved"].as_f64());
        let same = |got: Option<f64>, pin: Option<f64>| match (got, pin) {
            (Some(g), Some(p)) => (g - p).abs() <= PINNED * p.abs() + FLOOR,
            (None, None) => true,
            _ => false,
        };
        match found.get(&key) {
            None => problems.push(format!("{key:?} is listed but now agrees: remove it")),
            Some((got, moved, report)) => {
                if !same(*got, pinned.0) || !same(*moved, pinned.1) {
                    problems.push(format!(
                        "{key:?} is listed at {pinned:?} but changed: {report}"
                    ));
                }
            }
        }
        listed.push(key);
    }
    for (key, (_, _, report)) in &found {
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
