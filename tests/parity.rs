//! Parity with the Python implementation: every fixture the Python tests build, exported to STEP
//! by `tools/export_fixtures.py`, must give the same records, defining faces and evidence errors.

use std::path::{Path, PathBuf};

use quiddity::fillets::{FilletOptions, discover_fillets, recognise_fillets};
use quiddity::geom::SurfaceType;
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

/// The reader must walk faces in OpenCascade's order with the same surfaces and extents.
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
            (0..3).all(|k| (got[k] - want[k].as_f64().unwrap()).abs() <= 2e-3 + 1e-6 * got[k].abs())
        };
        if got != "OTHER" && (!close(b.min, &expected["min"]) || !close(b.max, &expected["max"])) {
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
        match (discover_fillets(part, &opts), run.get("evidence_error")) {
            (Err(_), Some(_)) => {}
            (Ok(found), None) => {
                let faces: Vec<u64> = found.iter().map(|o| o.face as u64).collect();
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
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
