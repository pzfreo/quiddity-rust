//! Replays every recogniser call the Python test suite makes (captured by
//! `tools/capture_plugin.py` into `tests/fixtures/captured/calls.json`) and compares the port's
//! answer with Python's, call by call. One test per recogniser, so a regression names its family.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quiddity::{Part, read_step_file};
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/captured")
}

fn load(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join(name)).unwrap()).unwrap()
}

fn replay(function: &str) {
    let calls = load("calls.json");
    let known: Vec<Value> = serde_json::from_value(load("known_divergences.json")).unwrap();
    let known: Vec<&str> = known
        .iter()
        .filter(|d| d["function"] == function)
        .map(|d| d["test"].as_str().unwrap())
        .collect();
    let mut parts: BTreeMap<String, Part> = BTreeMap::new();
    let (mut passed, mut expected, mut failures) = (0, 0, Vec::new());
    let calls: Vec<&Value> = calls["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["function"] == function)
        .collect();
    assert!(!calls.is_empty(), "no captured calls for {function}");
    for c in &calls {
        let got = match c["file"].as_str() {
            Some(file) => {
                let part = parts
                    .entry(file.to_owned())
                    .or_insert_with(|| read_step_file(&dir().join(file)).unwrap());
                common::recognise(function, part, &c["options"])
            }
            None => common::recognise_records(function, &c["arguments"]),
        };
        let test = c["test"].as_str().unwrap();
        if common::same(&got, &c["result"]) {
            passed += 1;
        } else if known.contains(&test) {
            expected += 1;
        } else {
            failures.push(format!(
                "{test} [{}] {}\n  rust   {got}\n  python {}",
                c["file"], c["options"], c["result"]
            ));
        }
    }
    eprintln!(
        "{function}: {passed} match, {expected} known divergences, {} unexpected",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} calls differ:\n{}",
        failures.len(),
        calls.len(),
        failures.join("\n")
    );
}

#[test]
fn fillets_match_python() {
    replay("recognise_fillets");
}

#[test]
fn countersinks_match_python() {
    replay("recognise_countersinks");
}

#[test]
fn holes_match_python() {
    replay("recognise_holes");
}

#[test]
fn hole_patterns_match_python() {
    replay("recognise_hole_patterns");
}

#[test]
fn chamfers_match_python() {
    replay("recognise_chamfers");
}
