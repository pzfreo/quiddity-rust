//! Python's local-degradation retry against the port (`tools/capture_local_degradation.py`,
//! `tests/fixtures/captured/local_degradation/capture.json`).
//!
//! Python's default inventory runs strictly and, when that refuses an unproved hole on a part
//! whose solids are not all valid, runs again with `local_degradation` set, where each family
//! skips a record that proves no one valid solid instead of refusing. The port's `recognise`
//! verifies no family's evidence, so it has no strict refusal to retry on (whether it should
//! panic or carry a refusal is an open maintainer question); what is compared here is each
//! family's evidence path:
//!
//! - which corpus parts reach the retry: in the port, the strict hole evidence path (countersinks
//!   composed, as the aggregate composes them) refusing for want of a valid solid on a part whose
//!   solids are not all valid;
//! - on every part Python retried, each record Python skipped: what the port's evidence path for
//!   that family does with it. Holes and pockets have the degraded path
//!   (`discover_locally_degraded`); the other families' strict paths answer `refused` (the whole family), `published` (a record on
//!   those faces) or `skipped` (no record there). A face is found by its box, so a part the port
//!   reads in another face order still compares.
//!
//! Every difference is listed in `captured/local_degradation/known.json` with a verdict and a
//! reason: `{"file", "kind": "retry", "rust"}` for a part only one side retries, `{"file",
//! "kind": "skip", "module", "function", "faces" (Python's), "rust"}` for a skip the port does
//! not reproduce. Unlisted, stale and doubly listed differences fail, and a failing run prints
//! the entries it needs.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use quiddity::Part;
use quiddity::features::evidence::EvidenceError;
use quiddity::features::{Context, countersinks, holes, pockets};
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const CAPTURE: &str = "captured/local_degradation/capture.json";
const KNOWN: &str = "captured/local_degradation/known.json";

fn corpus() -> Option<PathBuf> {
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
    corpus
}

fn parts(capture: &Value) -> &Vec<Value> {
    capture["parts"].as_array().unwrap()
}

/// Every difference must be listed once, and every listed one found.
fn check_known(found: Vec<Value>, kind: &str) {
    let known = common::load(KNOWN);
    let entries: Vec<Value> = known
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == kind)
        .cloned()
        .collect();
    common::check_verdicts("local_degradation/known.json", &entries);
    let key = |e: &Value| {
        let mut e = e.clone();
        let map = e.as_object_mut().unwrap();
        map.remove("verdict");
        map.remove("reason");
        e.to_string()
    };
    let mut listed: BTreeMap<String, bool> = BTreeMap::new();
    let mut problems = Vec::new();
    for e in &entries {
        if listed.insert(key(e), false).is_some() {
            problems.push(format!("listed twice: {e}"));
        }
    }
    let mut needed = Vec::new();
    for difference in found {
        match listed.get_mut(&key(&difference)) {
            Some(seen) => *seen = true,
            None => {
                problems.push(format!("unlisted: {difference}"));
                let mut entry = difference.clone();
                entry["verdict"] = json!("undetermined");
                entry["reason"] = json!("TODO");
                needed.push(entry);
            }
        }
    }
    problems.extend(
        listed
            .iter()
            .filter(|(_, seen)| !**seen)
            .map(|(k, _)| format!("stale: {k}")),
    );
    if !needed.is_empty() {
        problems.push(format!(
            "entries needed:\n{}",
            serde_json::to_string_pretty(&needed).unwrap()
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The capture is of this corpus, at the revision `corpus.json` records.
#[test]
fn capture_covers_the_corpus() {
    let capture = common::load(CAPTURE);
    let corpus = common::load("corpus.json");
    assert_eq!(capture["quiddity_revision"], corpus["quiddity_revision"]);
    let want: Vec<(&Value, &Value)> = corpus["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (&f["file"], &f["sha256"]))
        .collect();
    let got: Vec<(&Value, &Value)> = parts(&capture)
        .iter()
        .map(|p| (&p["file"], &p["sha256"]))
        .collect();
    assert_eq!(got, want);
    // Python retries exactly the parts whose strict pass refused the unproved hole, and the
    // retry completes every inventory.
    for p in parts(&capture) {
        let refused = p["strict"]["error"].as_str().is_some_and(|e| {
            e == format!("ValueError: {}", capture["retry_message"].as_str().unwrap())
        });
        assert_eq!(refused, p.get("degraded").is_some(), "{}", p["file"]);
        assert_eq!(p["result"], "ok", "{}", p["file"]);
    }
}

/// The strict and locally degraded hole evidence paths over one part, countersinks composed.
fn hole_paths(part: &Part) -> (Result<usize, EvidenceError>, Vec<Vec<usize>>) {
    let ctx = Context::new(part);
    let seats = countersinks::discover(&ctx);
    let strict = holes::discover_verified(&ctx, &seats).map(|found| found.len());
    let degraded = holes::discover_locally_degraded(&ctx, &seats)
        .unwrap_or_else(|e| panic!("locally degraded holes refused: {e}"))
        .into_iter()
        .map(|o| {
            let mut faces = o.defining;
            faces.sort_unstable();
            faces
        })
        .collect();
    (strict, degraded)
}

/// The locally degraded pocket evidence path over one part: each pocket's defining faces.
fn degraded_pockets(part: &Part) -> Result<Vec<Vec<usize>>, EvidenceError> {
    pockets::discover_locally_degraded(&Context::new(part)).map(|found| {
        found
            .into_iter()
            .map(|o| {
                let mut faces = o.defining;
                faces.sort_unstable();
                faces
            })
            .collect()
    })
}

/// Whether the port reaches the retry on a part: its strict hole evidence refused for want of a
/// valid solid, and its solids not all valid.
fn port_retries(part: &Part) -> bool {
    let all_valid =
        !part.solids.is_empty() && (0..part.solids.len()).all(|s| part.solid_is_valid(s));
    matches!(hole_paths(part).0, Err(EvidenceError::NoValidSolid)) && !all_valid
}

#[test]
fn retry_is_reached_where_python_reaches_it() {
    let Some(dir) = corpus() else { return };
    let capture = common::load(CAPTURE);
    let entries = parts(&capture);
    let found = common::parallel::map(entries, |p| {
        let file = p["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let python = p.get("degraded").is_some();
        let port = port_retries(&part);
        (python != port).then(|| json!({"file": file, "kind": "retry", "rust": port}))
    });
    let retried = entries
        .iter()
        .filter(|p| p.get("degraded").is_some())
        .count();
    eprintln!(
        "local degradation: Python retries {retried} of {} parts",
        entries.len()
    );
    check_known(found.into_iter().flatten().collect(), "retry");
}

/// The port's face with this Python face's box (to the inventory's 5e-3), if exactly one.
fn port_face(part: &Part, python: &Value) -> Option<usize> {
    let close = |got: [f64; 3], want: &Value| {
        (0..3).all(|k| (got[k] - want[k].as_f64().unwrap()).abs() <= 5e-3 + 1e-6 * got[k].abs())
    };
    let matches: Vec<usize> = (0..part.faces.len())
        .filter(|&f| {
            let b = part.face_bounds(f);
            close(b.min, &python["min"]) && close(b.max, &python["max"])
        })
        .collect();
    (matches.len() == 1).then(|| matches[0])
}

/// The Python entry point whose evidence path a skipping family function is part of; `None`
/// for one the port has no evidence path for (face levels and risers are measurements there).
fn entry_point(module: &str, function: &str) -> Option<&'static str> {
    Some(match (module, function) {
        ("holes", "_discover_holes") => "recognise_holes",
        ("fillets", "_discover_fillets") => "recognise_fillets",
        ("_recess_features", "_discover_pockets") => "recognise_pockets",
        ("plates", "_discover_plates") => "recognise_plates",
        ("levels", "_discover_step_levels" | "_discover_risers") => return None,
        other => panic!("{other:?}: no evidence path mapped for this skip"),
    })
}

/// What the port's evidence path does with the record whose proof faces are *faces*.
fn port_outcome(part: &Part, function: Option<&str>, faces: &[usize]) -> &'static str {
    let defining = match function {
        None => return "no evidence path",
        Some("recognise_holes") => Ok(hole_paths(part).1),
        Some("recognise_pockets") => degraded_pockets(part).map_err(|e| e.to_string()),
        Some(f) => common::defining(f, part, &json!({})),
    };
    match defining {
        Err(_) => "refused",
        Ok(found) => {
            // A record's own faces are among those its proof asked about (a plate's proof asks
            // face by face, so there the asked face is one of the record's).
            let on = |d: &Vec<usize>| {
                !d.is_empty()
                    && (d.iter().all(|f| faces.contains(f)) || faces.iter().all(|f| d.contains(f)))
            };
            if found.iter().any(on) {
                "published"
            } else {
                "skipped"
            }
        }
    }
}

#[test]
fn skips_agree_with_python() {
    let Some(dir) = corpus() else { return };
    let capture = common::load(CAPTURE);
    let retried: Vec<&Value> = parts(&capture)
        .iter()
        .filter(|p| p.get("degraded").is_some())
        .collect();
    let found = common::parallel::map(&retried, |p| {
        let file = p["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let mut out = Vec::new();
        for skip in p["skips"].as_array().unwrap() {
            let (module, function) = (
                skip["module"].as_str().unwrap(),
                skip["function"].as_str().unwrap(),
            );
            let faces: Option<Vec<usize>> = skip["boxes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| port_face(&part, b))
                .collect();
            let rust = match faces {
                None => "faces not found",
                Some(faces) => port_outcome(&part, entry_point(module, function), &faces),
            };
            if rust != "skipped" {
                out.push(json!({"file": file, "kind": "skip", "module": module,
                                "function": function, "faces": skip["faces"], "rust": rust}));
            }
        }
        out
    });
    let skips: usize = retried
        .iter()
        .map(|p| p["skips"].as_array().unwrap().len())
        .sum();
    eprintln!(
        "local degradation: {skips} records skipped on {} parts",
        retried.len()
    );
    check_known(found.into_iter().flatten().collect(), "skip");
}

/// Two of `tests/invariance.rs`'s rigid motions: one moving every axis and the origin, and one
/// reversing two axes.
const MOTIONS: [(&str, Placement); 2] = [
    (
        "rot_zx_moved",
        [
            [0.0, 0.0, 1.0, 123.456],
            [1.0, 0.0, 0.0, -78.9],
            [0.0, 1.0, 0.0, 41.3],
        ],
    ),
    (
        "rot_y180",
        [
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 0.0],
        ],
    ),
];

/// On every part Python retries, the locally degraded holes and pockets (and whether the port
/// retries) are the same faces however the part is placed.
#[test]
fn locally_degraded_paths_are_placement_independent() {
    let Some(dir) = corpus() else { return };
    let capture = common::load(CAPTURE);
    for p in parts(&capture)
        .iter()
        .filter(|p| p.get("degraded").is_some())
    {
        let path = dir.join(p["file"].as_str().unwrap());
        let base = read_step_file(&path).unwrap();
        let want = (
            port_retries(&base),
            hole_paths(&base).1,
            degraded_pockets(&base),
        );
        for (motion, placement) in &MOTIONS {
            let moved = read_step_file_placed(&path, placement).unwrap();
            let got = (
                port_retries(&moved),
                hole_paths(&moved).1,
                degraded_pockets(&moved),
            );
            assert_eq!(got, want, "{} {motion}", p["file"]);
        }
    }
}
