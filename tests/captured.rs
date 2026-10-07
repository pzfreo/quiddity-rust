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
    common::check_verdicts("captured/known_divergences.json", &known);
    let known: Vec<&Value> = known.iter().filter(|d| d["function"] == function).collect();
    // An entry names a test, optionally narrowed to one file and one option set.
    let covers = |d: &Value, c: &Value| {
        d["test"] == c["test"]
            && d.get("file").is_none_or(|f| *f == c["file"])
            && d.get("options").is_none_or(|o| *o == c["options"])
    };
    let mut used = vec![false; known.len()];
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
        } else if let Some(i) = known.iter().position(|d| covers(d, c)) {
            used[i] = true;
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
    let stale: Vec<&Value> = known
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(d, _)| *d)
        .collect();
    assert!(
        failures.is_empty() && stale.is_empty(),
        "{} of {} calls differ:\n{}\nknown divergences that no longer differ: {stale:?}",
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

#[test]
fn bosses_match_python() {
    replay("recognise_bosses");
}

#[test]
fn angled_steps_match_python() {
    replay("recognise_angled_steps");
}

#[test]
fn flats_match_python() {
    replay("recognise_flats");
}

#[test]
fn paired_ramp_steps_match_python() {
    replay("recognise_paired_ramp_steps");
}

#[test]
fn oriented_chamfers_match_python() {
    replay("recognise_oriented_chamfers");
}

#[test]
fn circular_face_patterns_match_python() {
    replay("recognise_circular_face_patterns");
}

#[test]
fn face_levels_match_python() {
    replay("recognise_face_levels");
}

#[test]
fn risers_match_python() {
    replay("recognise_risers");
}

#[test]
fn thin_wall_bodies_match_python() {
    replay("recognise_thin_wall_bodies");
}

#[test]
fn interior_voids_match_python() {
    replay("recognise_interior_voids");
}

#[test]
fn through_steps_match_python() {
    replay("recognise_through_steps");
}

#[test]
fn oblique_through_steps_match_python() {
    replay("recognise_oblique_through_steps");
}

#[test]
fn circular_blind_steps_match_python() {
    replay("recognise_circular_blind_steps");
}

#[test]
fn turned_steps_match_python() {
    replay("recognise_turned_steps");
}

#[test]
fn round_bottom_blind_slots_match_python() {
    replay("recognise_round_bottom_blind_slots");
}

#[test]
fn rectangular_blind_slots_match_python() {
    replay("recognise_rectangular_blind_slots");
}

#[test]
fn gusset_ribs_match_python() {
    replay("recognise_gusset_ribs");
}

#[test]
fn gusset_rib_patterns_match_python() {
    replay("recognise_gusset_rib_patterns");
}
