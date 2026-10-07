//! Parity over the shared STEP corpus: for every corpus file, the face inventory, the kernel
//! answers recognisers lean on (and solid masses) and each ported recogniser's answer must match
//! what `tools/export_corpus.py` recorded from Python, except for the differences listed (with
//! reasons) in `tests/fixtures/known_divergences.json`.

mod common;

use std::path::{Path, PathBuf};

use quiddity::read_step_file;
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join(name)).unwrap()).unwrap()
}

fn corpus_dir() -> Option<PathBuf> {
    let dir = std::env::var("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../quiddity/tests/corpus"));
    dir.is_dir().then_some(dir)
}

#[test]
fn corpus_matches_python() {
    let Some(dir) = corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let mut problems = Vec::new();
    let mut counts = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for entry in load("corpus.json")["files"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap();
        let part = match read_step_file(&dir.join(name)) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!("{name}: read failed: {e}"));
                continue;
            }
        };
        let before = problems.len();
        common::check_inventory(
            name,
            &part,
            entry["inventory"].as_array().unwrap(),
            &mut problems,
        );
        // Kernel answers are per face index, meaningless once the inventories disagree.
        if problems.len() == before {
            check_kernel(name, &part, &entry["kernel"], &mut problems);
        }
        for (function, runs) in entry["results"].as_object().unwrap() {
            for run in runs.as_array().unwrap() {
                let tally = counts.entry(function.clone()).or_default();
                let got = common::recognise(function, &part, &run["options"]);
                if !common::same(&got, &run["result"]) {
                    tally.1 += 1;
                    problems.push(format!(
                        "{name} {function} {}: records differ: {}",
                        run["options"],
                        common::diff(&got, &run["result"])
                    ));
                    continue;
                }
                tally.0 += 1;
                if run.get("defining").is_some() || run.get("evidence_error").is_some() {
                    check_evidence(name, function, &part, run, &mut problems);
                }
            }
        }
    }
    for (function, (ok, bad)) in &counts {
        eprintln!("{function}: {ok} runs match, {bad} differ");
    }
    let known: Vec<Value> = serde_json::from_value(load("known_divergences.json")).unwrap();
    let is_known = |p: &str, d: &Value| {
        p.starts_with(&format!("{} ", d["file"].as_str().unwrap()))
            || p.starts_with(&format!("{}:", d["file"].as_str().unwrap()))
    };
    // `contains` is one string or a list of strings the problem must all contain.
    let matches = |p: &str, d: &Value| {
        is_known(p, d)
            && match &d["contains"] {
                Value::Array(all) => all.iter().all(|c| p.contains(c.as_str().unwrap())),
                one => p.contains(one.as_str().unwrap()),
            }
    };
    let unexpected: Vec<&String> = problems
        .iter()
        .filter(|p| !known.iter().any(|d| matches(p, d)))
        .collect();
    let stale: Vec<&Value> = known
        .iter()
        .filter(|d| !problems.iter().any(|p| matches(p, d)))
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

/// The defining faces (or the refusal) of the evidence path agree with Python's.
fn check_evidence(
    name: &str,
    function: &str,
    part: &quiddity::Part,
    run: &Value,
    problems: &mut Vec<String>,
) {
    let ours = common::defining(function, part, &run["options"]);
    match (ours, run.get("evidence_error"), run.get("defining")) {
        (Err(_), Some(_), _) => {}
        (Ok(faces), None, Some(want)) => {
            let want: Vec<Vec<usize>> = serde_json::from_value(want.clone()).unwrap();
            if faces != want {
                problems.push(format!(
                    "{name} {function} {}: evidence faces {faces:?}, Python {want:?}",
                    run["options"]
                ));
            }
        }
        (got, want, _) => problems.push(format!(
            "{name} {function} {}: evidence {got:?}, Python {want:?}",
            run["options"]
        )),
    }
}

/// Each solid's volume and area, each face's UV range (`BRepTools::UVBounds`) and the arc
/// between each pair of neighbours agree with OpenCascade's. One problem per file and query,
/// naming the first few faces.
///
/// Masses agree to 1e-9 except where OpenCascade integrates approximations (B-spline pcurves)
/// or the file's boundary does not close; the problem names the worst relative difference as a
/// power of ten, so a known divergence pins how far apart the answers may be.
fn check_kernel(name: &str, part: &quiddity::Part, kernel: &Value, problems: &mut Vec<String>) {
    let want = kernel["solids"].as_array().unwrap();
    if want.len() != part.solids.len() {
        problems.push(format!(
            "{name}: {} solids, OpenCascade has {}",
            part.solids.len(),
            want.len()
        ));
    } else {
        let mut worst: f64 = 0.0;
        for (s, w) in want.iter().enumerate() {
            let (v, a) = (w[0].as_f64().unwrap(), w[1].as_f64().unwrap());
            worst = match part.solid_mass(s) {
                Some((gv, ga)) => worst
                    .max((gv - v).abs() / v.abs())
                    .max((ga - a).abs() / a.abs()),
                None => f64::INFINITY,
            };
        }
        if worst > 1e-9 {
            problems.push(format!(
                "{name}: solid masses differ by up to 1e{}",
                worst.log10().ceil()
            ));
        }
    }
    let mut report = |what: &str, bad: Vec<String>| {
        if !bad.is_empty() {
            problems.push(format!(
                "{name}: {what} differ on {}: {}",
                bad.len(),
                bad[..bad.len().min(3)].join("; ")
            ));
        }
    };
    let mut bad = Vec::new();
    for (face, want) in kernel["uv_bounds"].as_array().unwrap().iter().enumerate() {
        let got = part
            .uv_bounds(face)
            .map(|(u0, u1, v0, v1)| [u0, u1, v0, v1]);
        let got = serde_json::to_value(got).unwrap();
        if !common::same(&got, want) {
            let kind = format!("{:?}", part.faces[face].surface.kind());
            bad.push(format!("face {face} ({kind}) {got} vs {want}"));
        }
    }
    report("uv_bounds", bad);
    let mut bad = Vec::new();
    for pair in kernel["arcs"].as_array().unwrap() {
        let (a, b) = (
            pair[0].as_u64().unwrap() as usize,
            pair[1].as_u64().unwrap() as usize,
        );
        let got = part.arc(a, b).map(|arc| format!("{arc:?}").to_lowercase());
        if got.as_deref() != pair[2].as_str() {
            bad.push(format!("faces {a}-{b} {got:?} vs {}", pair[2]));
        }
    }
    report("arcs", bad);
}
