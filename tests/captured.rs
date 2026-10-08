//! Replays every recogniser call the Python test suite makes (captured by
//! `tools/capture_plugin.py` into `tests/fixtures/captured/calls.json`) and compares the port's
//! answer with Python's, call by call. One test per recogniser, so a regression names its family.
//!
//! A difference is listed in `tests/fixtures/captured/known_divergences.json` by the exact call it
//! explains and how many such calls diverge:
//!
//! ```json
//! {"function": "recognise_holes", "test": "tests/test_x.py::test_y[1]", "file": "abcd.step.gz",
//!  "options": {}, "count": 1, "verdict": "rust-correct", "reason": "..."}
//! ```
//!
//! (`file` is `null` for a call on records.) A failure prints each unlisted call as the entry it
//! needs.

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

/// Calls the Python suite makes that the capture could not record (`skipped` in `calls.json`,
/// each with its test node and reason), per recogniser. They are outside every replay, so the
/// counts are pinned: a change means the coverage changed and the README table with it.
const SKIPPED: &[(&str, usize)] = &[
    ("recognise_circular_blind_steps", 2),
    ("recognise_channels", 3),
    ("recognise_double_d_bores", 1),
    ("recognise_edge_open_prismatic_recesses", 2),
    ("recognise_grooves", 3),
    ("recognise_interior_voids", 2),
    ("recognise_oblique_through_steps", 1),
    ("recognise_oriented_slots", 1),
    ("recognise_plates", 4),
    ("recognise_pockets", 1),
    ("recognise_polygonal_bosses", 4),
    ("recognise_polygonal_stock", 1),
    ("recognise_prismatic_pockets", 6),
    ("recognise_rectangular_blind_slots", 1),
    ("recognise_rectangular_pads", 5),
    ("recognise_repeating_radial_profiles", 1),
    ("recognise_round_bottom_blind_slots", 1),
    ("recognise_through_steps", 7),
    ("recognise_turned_steps", 18),
];

/// The calls of *function* that the capture skipped: entries `{function, test, reason}` (one per
/// call), entries carried over from a capture made before the plugin kept test ids
/// `{function, test: null, reason, count}`, or that older capture's own form, counts keyed
/// `"<function>: <reason>"`.
fn skipped(function: &str) -> usize {
    match &load("calls.json")["skipped"] {
        Value::Array(all) => all
            .iter()
            .filter(|s| s["function"] == function)
            .map(|s| s.get("count").map_or(1, |n| n.as_u64().unwrap() as usize))
            .sum(),
        Value::Object(counts) => counts
            .iter()
            .filter(|(k, _)| k.split(':').next() == Some(function))
            .map(|(_, n)| n.as_u64().unwrap() as usize)
            .sum(),
        other => panic!("calls.json: unreadable `skipped`: {other}"),
    }
}

/// What a listed difference names: one captured call's test node, file (or `null` for a call on
/// records) and options, and how many calls with that identity diverge (a test may make the same
/// call more than once). Each diverging call must be named by exactly one entry, and each entry
/// must name exactly `count` diverging calls.
fn identity(d: &Value) -> (&Value, &Value, &Value) {
    for k in ["test", "file", "options", "count"] {
        assert!(
            d.get(k).is_some(),
            "captured/known_divergences.json: entry without {k}: {d}"
        );
    }
    (&d["test"], &d["file"], &d["options"])
}

fn replay(function: &str) {
    let calls = load("calls.json");
    let known: Vec<Value> = serde_json::from_value(load("known_divergences.json")).unwrap();
    common::check_verdicts("captured/known_divergences.json", &known);
    let known: Vec<&Value> = known.iter().filter(|d| d["function"] == function).collect();
    for (i, d) in known.iter().enumerate() {
        assert!(
            known[..i].iter().all(|e| identity(e) != identity(d)),
            "captured/known_divergences.json: two entries for one call: {d}"
        );
    }
    let mut used = vec![0; known.len()];
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
        if common::same(&got, &c["result"]) {
            passed += 1;
        } else if let Some(i) = known
            .iter()
            .position(|d| identity(d) == (&c["test"], &c["file"], &c["options"]))
        {
            used[i] += 1;
            expected += 1;
        } else {
            let entry = serde_json::json!({
                "function": function, "test": c["test"], "file": c["file"],
                "options": c["options"], "count": 1
            });
            failures.push(format!("{entry}\n  rust   {got}\n  python {}", c["result"]));
        }
    }
    let skipped = skipped(function);
    eprintln!(
        "{function}: {passed} match, {expected} known divergences, {} unexpected, {skipped} not \
         captured",
        failures.len()
    );
    let miscounted: Vec<String> = known
        .iter()
        .zip(&used)
        .filter(|(d, n)| d["count"].as_u64() != Some(**n as u64))
        .map(|(d, n)| format!("{n} found: {d}"))
        .collect();
    let pinned = SKIPPED
        .iter()
        .find(|(f, _)| *f == function)
        .map_or(0, |(_, n)| *n);
    assert!(
        failures.is_empty() && miscounted.is_empty() && skipped == pinned,
        "{} of {} calls differ (as entries, verdict and reason to be added):\n{}\nknown \
         divergences whose count changed: {miscounted:#?}\ncalls Python made that the capture \
         could not record: {skipped}, pinned {pinned}",
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
fn double_d_bores_match_python() {
    replay("recognise_double_d_bores");
}

#[test]
fn gusset_rib_patterns_match_python() {
    replay("recognise_gusset_rib_patterns");
}

#[test]
fn grooves_match_python() {
    replay("recognise_grooves");
}

#[test]
fn plates_match_python() {
    replay("recognise_plates");
}

#[test]
fn edge_open_circular_pockets_match_python() {
    replay("recognise_edge_open_circular_pockets");
}

#[test]
fn edge_open_prismatic_recesses_match_python() {
    replay("recognise_edge_open_prismatic_recesses");
}

#[test]
fn blends_match_python() {
    replay("recognise_blends");
}

#[test]
fn sheet_metal_bodies_match_python() {
    replay("recognise_sheet_metal_bodies");
}

#[test]
fn slots_match_python() {
    replay("recognise_slots");
}

#[test]
fn pockets_match_python() {
    replay("recognise_pockets");
}

#[test]
fn channels_match_python() {
    replay("recognise_channels");
}

#[test]
fn slot_patterns_match_python() {
    replay("recognise_slot_patterns");
}

#[test]
fn pocket_patterns_match_python() {
    replay("recognise_pocket_patterns");
}

#[test]
fn repeating_radial_profiles_match_python() {
    replay("recognise_repeating_radial_profiles");
}

#[test]
fn freeform_surfaces_match_python() {
    replay("recognise_freeform_surfaces");
}

#[test]
fn polygonal_bosses_match_python() {
    replay("recognise_polygonal_bosses");
}

#[test]
fn polygonal_stock_match_python() {
    replay("recognise_polygonal_stock");
}

#[test]
fn prismatic_pockets_match_python() {
    replay("recognise_prismatic_pockets");
}

#[test]
fn oriented_slots_match_python() {
    replay("recognise_oriented_slots");
}

#[test]
fn oriented_slot_patterns_match_python() {
    replay("recognise_oriented_slot_patterns");
}

#[test]
fn rectangular_pads_match_python() {
    replay("recognise_rectangular_pads");
}
