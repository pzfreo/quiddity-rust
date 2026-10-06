//! Parity with the Python implementation: every fixture the Python tests build, exported to STEP
//! by `tools/export_fixtures.py`, must give the same records, defining faces and evidence errors.

use std::path::{Path, PathBuf};

use quiddity::features::Context;
use quiddity::features::fillets::{FilletOptions, discover_verified, recognise_fillets};
use quiddity::kernel::geom::SurfaceType;
use quiddity::{Part, read_step_file};
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn manifest() -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join("manifest.json")).unwrap())
        .unwrap()
}

fn corpus_dir() -> Option<PathBuf> {
    let dir = std::env::var("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../quiddity/tests/corpus"));
    dir.is_dir().then_some(dir)
}

fn options(v: &Value) -> FilletOptions {
    let mut o = FilletOptions::default();
    if let Some(m) = v.get("min_radius").and_then(Value::as_f64) {
        o.min_radius = Some(m);
    }
    if let Some(f) = v.get("max_radius_frac").and_then(Value::as_f64) {
        o.max_radius_frac = f;
    }
    if let Some(c) = v.get("include_cylindrical").and_then(Value::as_bool) {
        o.include_cylindrical = c;
    }
    o
}

fn type_name(t: SurfaceType) -> &'static str {
    match t {
        SurfaceType::Plane => "PLANE",
        SurfaceType::Cylinder => "CYLINDER",
        SurfaceType::Cone => "CONE",
        SurfaceType::Sphere => "SPHERE",
        SurfaceType::Torus => "TORUS",
        SurfaceType::Freeform | SurfaceType::Other => "OTHER",
    }
}

/// The reader must walk faces in OpenCascade's order with the same surfaces and extents. The
/// extents are a fingerprint, not a parity target: OpenCascade's optimal boxes overshoot curved
/// B-spline boundaries by a few microns, hence the 5e-3 band.
fn check_inventory(name: &str, part: &Part, inventory: &[Value], problems: &mut Vec<String>) {
    if part.faces.len() != inventory.len() {
        problems.push(format!(
            "{name}: {} faces, Python has {}",
            part.faces.len(),
            inventory.len()
        ));
        return;
    }
    for (i, expected) in inventory.iter().enumerate() {
        let want = expected["type"].as_str().unwrap();
        let got = type_name(part.faces[i].surface.kind());
        if got != want
            && !(got == "OTHER"
                && !["PLANE", "CYLINDER", "CONE", "SPHERE", "TORUS"].contains(&want))
        {
            problems.push(format!("{name}: face {i} is {got}, Python has {want}"));
            return;
        }
        let b = part.face_bounds(i);
        let close = |got: [f64; 3], want: &Value| {
            (0..3).all(|k| (got[k] - want[k].as_f64().unwrap()).abs() <= 5e-3 + 1e-6 * got[k].abs())
        };
        // Sphere patches through a pole get approximate interior extremes (see README).
        let fingerprinted = got != "OTHER" && got != "SPHERE";
        if fingerprinted && (!close(b.min, &expected["min"]) || !close(b.max, &expected["max"])) {
            problems.push(format!(
                "{name}: face {i} ({got}) bounds {:?}..{:?}, Python {}..{}",
                b.min, b.max, expected["min"], expected["max"]
            ));
            return;
        }
    }
}

fn check_runs(name: &str, part: &Part, runs: &[Value], problems: &mut Vec<String>) {
    for run in runs {
        let opts = options(&run["options"]);
        let got: Vec<Value> = recognise_fillets(part, &opts)
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect();
        let want = run["records"].as_array().unwrap();
        let same = got.len() == want.len()
            && got.iter().zip(want).all(|(g, w)| {
                g["axis"] == w["axis"]
                    && g["turned"] == w["turned"]
                    && g["side"] == w["side"]
                    && g["radius"].as_f64() == w["radius"].as_f64()
                    && (0..3).all(|k| g["at"][k].as_f64() == w["at"][k].as_f64())
            });
        if !same {
            let brief = |r: &Value| {
                format!(
                    "{}{} r{} @{}",
                    r["axis"].as_str().unwrap(),
                    if r["turned"].as_bool().unwrap() {
                        "T"
                    } else {
                        ""
                    },
                    r["radius"],
                    r["at"]
                )
            };
            let g: Vec<String> = got.iter().map(brief).collect();
            let w: Vec<String> = want.iter().map(brief).collect();
            let only_rust: Vec<&String> = g.iter().filter(|x| !w.contains(x)).collect();
            let only_python: Vec<&String> = w.iter().filter(|x| !g.contains(x)).collect();
            problems.push(format!(
                "{name} {}: {} records vs Python {}; only rust {only_rust:?}; only python {only_python:?}",
                run["options"],
                g.len(),
                w.len()
            ));
            continue;
        }
        match (
            discover_verified(&Context::new(part), &opts),
            run.get("evidence_error"),
        ) {
            (Err(_), Some(_)) => {}
            (Ok(found), None) => {
                let faces: Vec<u64> = found.iter().map(|o| o.defining[0] as u64).collect();
                let want: Vec<u64> = run["defining"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| d["face"].as_u64().unwrap())
                    .collect();
                if faces != want {
                    problems.push(format!(
                        "{name} {}: defining faces {faces:?}, Python {want:?}",
                        run["options"]
                    ));
                }
            }
            (got, want) => problems.push(format!(
                "{name} {}: evidence {got:?}, Python {want:?}",
                run["options"]
            )),
        }
    }
}

#[test]
fn built_fixtures_match_python() {
    let manifest = manifest();
    let mut problems = Vec::new();
    for entry in manifest["built"].as_array().unwrap() {
        let name = entry["name"].as_str().unwrap();
        let part = read_step_file(&fixtures().join(entry["file"].as_str().unwrap())).unwrap();
        check_inventory(
            name,
            &part,
            entry["inventory"].as_array().unwrap(),
            &mut problems,
        );
        check_runs(
            name,
            &part,
            entry["runs"].as_array().unwrap(),
            &mut problems,
        );
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// Corpus differences that are understood and recorded with their reasons. A problem is known
/// when it names a listed file and kind; a listed entry that no longer occurs is itself a
/// failure, so the list cannot go stale.
fn known_divergences() -> Vec<(String, String)> {
    let raw = std::fs::read_to_string(fixtures().join("known_divergences.json")).unwrap();
    let list: Vec<Value> = serde_json::from_str(&raw).unwrap();
    list.iter()
        .map(|d| {
            (
                d["file"].as_str().unwrap().to_owned(),
                d["problem"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn problem_kind(problem: &str) -> &'static str {
    if problem.contains(": evidence") {
        "evidence"
    } else if problem.contains("records vs Python") {
        "records"
    } else if problem.contains("faces, Python") {
        "faces"
    } else if problem.contains("bounds") {
        "bounds"
    } else {
        "other"
    }
}

fn names(problem: &str, file: &str) -> bool {
    problem.starts_with(&format!("{file}:")) || problem.starts_with(&format!("{file} "))
}

#[test]
fn corpus_matches_python() {
    let Some(dir) = corpus_dir() else {
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let manifest = manifest();
    let mut problems = Vec::new();
    for entry in manifest["corpus"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap();
        let part = match read_step_file(&dir.join(name)) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!("{name}: read failed: {e}"));
                continue;
            }
        };
        check_inventory(
            name,
            &part,
            entry["inventory"].as_array().unwrap(),
            &mut problems,
        );
        check_runs(
            name,
            &part,
            entry["runs"].as_array().unwrap(),
            &mut problems,
        );
    }
    let known = known_divergences();
    let is_known = |p: &String| {
        known
            .iter()
            .any(|(file, kind)| names(p, file) && problem_kind(p) == kind)
    };
    let unexpected: Vec<&String> = problems.iter().filter(|p| !is_known(p)).collect();
    let stale: Vec<&(String, String)> = known
        .iter()
        .filter(|(file, kind)| {
            !problems
                .iter()
                .any(|p| names(p, file) && problem_kind(p) == kind)
        })
        .collect();
    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "{} unexpected problems:\n{}\nstale known divergences: {stale:?}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
