//! Fillet evidence parity: hand-built cases exported by `tools/export_fixtures.py` must give the
//! same records, the same defining faces and the same evidence refusals as Python, with faces
//! walked in OpenCascade's order.

mod common;

use std::path::{Path, PathBuf};

use quiddity::features::Context;
use quiddity::features::fillets::{FilletOptions, discover_verified, recognise_fillets};
use quiddity::{Part, read_step_file};
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn manifest() -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join("manifest.json")).unwrap())
        .unwrap()
}

fn check_runs(name: &str, part: &Part, runs: &[Value], problems: &mut Vec<String>) {
    for run in runs {
        let opts: FilletOptions = common::options(&run["options"]);
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
        common::check_inventory(
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
