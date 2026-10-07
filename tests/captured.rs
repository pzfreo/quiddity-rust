//! Replays every recogniser call the Python test suite makes (captured by
//! `tools/capture_plugin.py` into `tests/fixtures/captured/calls.json`) and compares the port's
//! answer with Python's, call by call. One test per recogniser, so a regression names its family.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quiddity::features::countersinks::recognise_countersinks;
use quiddity::features::fillets::{FilletOptions, recognise_fillets};
use quiddity::features::hole_patterns::recognise_hole_patterns;
use quiddity::features::holes::{HoleOptions, HoleRecord, recognise_holes};
use quiddity::{Part, read_step_file};
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/captured")
}

fn calls(function: &str) -> Vec<Value> {
    let raw = std::fs::read_to_string(dir().join("calls.json")).unwrap();
    let all: Value = serde_json::from_str(&raw).unwrap();
    all["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["function"] == function)
        .cloned()
        .collect()
}

/// Known, explained differences: `{"function", "test", "reason"}` entries.
fn known(function: &str) -> Vec<String> {
    let raw = std::fs::read_to_string(dir().join("known_divergences.json")).unwrap();
    let list: Vec<Value> = serde_json::from_str(&raw).unwrap();
    list.iter()
        .filter(|d| d["function"] == function)
        .map(|d| d["test"].as_str().unwrap().to_owned())
        .collect()
}

/// Structural equality with Python's float semantics (-0.0 == 0.0), measured values agreeing to
/// one part in a million: below that, the two kernels' parameter-range arithmetic differs in its
/// last digits (sub-micron at part scale). Rounded fields still compare exactly at their grid.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// How a recogniser is called: on a part with options, or on records (a derived recogniser).
enum Call<'f> {
    Part(&'f dyn Fn(&Part, &Value) -> Value),
    Records(&'f dyn Fn(&Value) -> Value),
}

fn replay(function: &str, call: Call<'_>) {
    let known = known(function);
    let mut parts: BTreeMap<String, Part> = BTreeMap::new();
    let (mut passed, mut failures, mut expected) = (0, Vec::new(), 0);
    let all = calls(function);
    assert!(!all.is_empty(), "no captured calls for {function}");
    for c in &all {
        let got = match &call {
            Call::Part(recognise) => {
                let file = c["file"].as_str().unwrap();
                let part = parts
                    .entry(file.to_owned())
                    .or_insert_with(|| read_step_file(&dir().join(file)).unwrap());
                recognise(part, &c["options"])
            }
            Call::Records(recognise) => recognise(&c["arguments"]),
        };
        let test = c["test"].as_str().unwrap();
        if same(&got, &c["result"]) {
            passed += 1;
        } else if known.iter().any(|k| k == test) {
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
        all.len(),
        failures.join("\n")
    );
}

#[test]
fn fillets_match_python() {
    replay(
        "recognise_fillets",
        Call::Part(&|part, o| {
            let mut opts = FilletOptions::default();
            if let Some(m) = o.get("min_radius").and_then(Value::as_f64) {
                opts.min_radius = Some(m);
            }
            if let Some(f) = o.get("max_radius_frac").and_then(Value::as_f64) {
                opts.max_radius_frac = f;
            }
            if let Some(c) = o.get("include_cylindrical").and_then(Value::as_bool) {
                opts.include_cylindrical = c;
            }
            serde_json::to_value(recognise_fillets(part, &opts)).unwrap()
        }),
    );
}

#[test]
fn countersinks_match_python() {
    replay(
        "recognise_countersinks",
        Call::Part(&|part, _| serde_json::to_value(recognise_countersinks(part)).unwrap()),
    );
}

#[test]
fn holes_match_python() {
    replay(
        "recognise_holes",
        Call::Part(&|part, o| {
            let opts = HoleOptions {
                with_countersinks: o.get("csinks").is_some_and(|c| c == "auto"),
            };
            serde_json::to_value(recognise_holes(part, &opts)).unwrap()
        }),
    );
}

#[test]
fn hole_patterns_match_python() {
    replay(
        "recognise_hole_patterns",
        Call::Records(&|arguments| {
            let holes: Vec<HoleRecord> = serde_json::from_value(arguments[0].clone()).unwrap();
            serde_json::to_value(recognise_hole_patterns(&holes)).unwrap()
        }),
    );
}
