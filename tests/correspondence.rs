//! Revision correspondence (`docs/correspondence.md`, "Tests").
//!
//! - Invariance: every corpus part, read under each of `tests/invariance.rs`'s rigid motions,
//!   corresponds with its unmoved self with every feature and every face carried, each to the
//!   very feature (same defining faces) and face (same index) it is.
//! - Revision pairs: build123d parts revised as a designer would (`tools/capture_revisions.py`,
//!   `tests/fixtures/revisions/`), each feature's class checked against the expected one.
//!
//! Differences are listed with verdicts in `tests/fixtures/known_correspondence.json`; the test
//! fails on an unlisted one and on a listed one that has gone.

mod common;

use std::collections::BTreeMap;

use quiddity::correspondence::{self, Class, Correspondence, Fingerprints};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use serde_json::Value;

/// `tests/invariance.rs`'s motions: a non-round translation and five rotations with exact ±1/0
/// entries.
const T: [f64; 3] = [123.456, -78.9, 41.3];
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);
const MOTIONS: [Motion; 6] = [
    (
        "translate",
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        T,
    ),
    (
        "rot_z90",
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0; 3],
    ),
    (
        "rot_x90",
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_y180",
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0; 3],
    ),
    (
        "cycle_xyz",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_zx_moved",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        T,
    ),
];

fn placement(r: &[[f64; 3]; 3], t: &[f64; 3]) -> Placement {
    [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]])
}

fn faces_of<'a>(f: &'a Fingerprints, id: &str) -> &'a [usize] {
    &f.features.iter().find(|x| x.id == id).unwrap().faces
}

/// The problems with a correspondence of a part with itself moved: anything not carried to
/// itself. Families whose recognition is listed as not invariant (`known_invariance.json`) are
/// left out, with the patterns made of them.
fn invariance_problems(
    old: &Fingerprints,
    new: &Fingerprints,
    c: &Correspondence,
    skip: &[String],
) -> Vec<String> {
    let skipped = |id: &str| {
        let family = id.split('/').next().unwrap();
        skip.iter()
            .any(|s| family == s || (s == "holes" && family == "hole_patterns"))
    };
    let mut out = Vec::new();
    for e in &c.features {
        if skipped(&e.old) {
            continue;
        }
        let ok = e.class == Class::Carried
            && e.new
                .as_deref()
                .is_some_and(|n| faces_of(new, n) == faces_of(old, &e.old));
        if !ok {
            out.push(format!(
                "feature {} {:?} → {:?} {:?} {:?}",
                e.old, e.class, e.new, e.candidates, e.changes
            ));
        }
    }
    for id in &c.new_features {
        if !skipped(id) {
            out.push(format!("feature {id} new"));
        }
    }
    let faces: Vec<String> = c
        .faces
        .iter()
        .filter(|e| !(e.class == Class::Carried && e.new == Some(e.old)))
        .map(|e| format!("face {} {:?} → {:?} {:?}", e.old, e.class, e.new, e.changes))
        .collect();
    if !faces.is_empty() {
        out.push(format!(
            "{} faces not carried to themselves, e.g. {}",
            faces.len(),
            faces[0]
        ));
    }
    out
}

fn known() -> Vec<Value> {
    let known = common::load("known_correspondence.json");
    let known = known.as_array().unwrap().clone();
    common::check_verdicts("known_correspondence.json", &known);
    known
}

#[test]
fn corpus_parts_correspond_with_themselves_moved() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let known: Vec<(String, String)> = known()
        .iter()
        .filter(|k| k["test"] == "invariance")
        .map(|k| {
            (
                k["file"].as_str().unwrap().to_string(),
                k["motion"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let invariance = common::load("known_invariance.json");
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    let (mut pairs, mut features, mut faces) = (0, 0, 0);
    for entry in common::load("corpus.json")["files"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap();
        let path = dir.join(name);
        let Ok(base) = read_step_file_placed(&path, &IDENTITY) else {
            continue; // the corpus test reports read failures
        };
        let old = correspondence::recognise(&base).fingerprints;
        for (motion, r, t) in &MOTIONS {
            let moved = read_step_file_placed(&path, &placement(r, t)).unwrap();
            let new = correspondence::recognise(&moved).fingerprints;
            let c = correspondence::correspond(&old, &new).unwrap();
            let skip: Vec<String> = invariance
                .as_array()
                .unwrap()
                .iter()
                .filter(|k| k["file"] == name && k["motion"] == *motion)
                .map(|k| k["family"].as_str().unwrap().to_string())
                .collect();
            let found = invariance_problems(&old, &new, &c, &skip);
            pairs += 1;
            features += old.features.len();
            faces += old.faces.len();
            let key = (name.to_string(), motion.to_string());
            if known.contains(&key) {
                if found.is_empty() {
                    problems.push(format!("{key:?} is listed but now carried: remove it"));
                }
                seen.push(key);
                continue;
            }
            if !found.is_empty() {
                problems.push(format!(
                    "{name} {motion} (aligned {}, symmetric {}): {}",
                    c.alignment.found,
                    c.alignment.symmetric,
                    found.join("; ")
                ));
            }
        }
    }
    for key in &known {
        if !seen.contains(key) {
            problems.push(format!("{key:?} is listed but was not checked"));
        }
    }
    eprintln!("{pairs} moved parts: {features} features, {faces} faces");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The revision pairs: each case's expected class for named features, by the feature's
/// family and position (fixture `expected.json`, written by `tools/capture_revisions.py`).
#[test]
fn revision_pairs_have_their_expected_classes() {
    let root = common::fixtures().join("revisions");
    let cases = common::load("revisions/expected.json");
    let known: Vec<(String, String)> = known()
        .iter()
        .filter(|k| k["test"] == "revisions")
        .map(|k| {
            (
                k["case"].as_str().unwrap().to_string(),
                k["check"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    let mut checked = 0;
    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let read = |which: &str| {
            let part =
                quiddity::read_step_file(&root.join(format!("{name}.{which}.step.gz"))).unwrap();
            correspondence::recognise(&part).fingerprints
        };
        let (old, new) = (read("old"), read("new"));
        let c = correspondence::correspond(&old, &new).unwrap();
        let mut results: Vec<(String, bool, String)> = Vec::new();
        // Alignment.
        let aligned = case["aligned"].as_bool().unwrap();
        results.push((
            "aligned".into(),
            c.alignment.found == aligned,
            format!("expected aligned {aligned}, got {:?}", c.alignment),
        ));
        // Each expectation names a feature by family and nearest old (or new) position.
        for exp in case["expect"].as_array().unwrap() {
            let family = exp["family"].as_str().unwrap();
            let class = exp["class"].as_str().unwrap();
            let label = exp["label"].as_str().unwrap();
            let check = format!("{label}: {family} {class}");
            let (ok, got) = check_expectation(&old, &new, &c, exp);
            results.push((check, ok, got));
        }
        // Every other feature: carried.
        let expected_ids = expected_ids(&old, &new, case);
        for e in &c.features {
            if expected_ids.contains(&e.old) {
                continue;
            }
            results.push((
                format!("other {}: carried", e.old),
                e.class == Class::Carried,
                format!("{:?} {:?} {:?}", e.class, e.candidates, e.changes),
            ));
        }
        for id in &c.new_features {
            if !expected_ids.contains(id) {
                results.push((format!("other new {id}"), false, "unexpectedly new".into()));
            }
        }
        for (check, ok, got) in results {
            checked += 1;
            let key = (name.to_string(), check.clone());
            if known.contains(&key) {
                if ok {
                    problems.push(format!("{key:?} is listed but now holds: remove it"));
                }
                seen.push(key);
            } else if !ok {
                problems.push(format!("{name}: {check}: {got}"));
            }
        }
    }
    for key in &known {
        if !seen.contains(key) {
            problems.push(format!("{key:?} is listed but was not checked"));
        }
    }
    eprintln!("{checked} revision checks");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The feature of *family* in *f* nearest *at*.
fn nearest(f: &Fingerprints, family: &str, at: &Value) -> Option<String> {
    let at: Vec<f64> = at.as_array()?.iter().map(|x| x.as_f64().unwrap()).collect();
    f.features
        .iter()
        .filter(|x| x.family == family && x.position.is_some())
        .min_by(|a, b| {
            let d = |p: [f64; 3]| (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f64>();
            d(a.position.unwrap()).total_cmp(&d(b.position.unwrap()))
        })
        .map(|x| x.id.clone())
}

fn expected_ids(old: &Fingerprints, new: &Fingerprints, case: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for exp in case["expect"].as_array().unwrap() {
        let family = exp["family"].as_str().unwrap();
        if let Some(id) = nearest(old, family, &exp["old_at"]) {
            out.push(id);
        }
        if let Some(id) = nearest(new, family, &exp["new_at"]) {
            out.push(id);
        }
        for at in exp["also"].as_array().into_iter().flatten() {
            if let Some(id) = nearest(old, family, at) {
                out.push(id);
            }
        }
    }
    out
}

/// Whether the correspondence gives the expectation's feature its class (and, where given, its
/// partner and changed fields).
fn check_expectation(
    old: &Fingerprints,
    new: &Fingerprints,
    c: &Correspondence,
    exp: &Value,
) -> (bool, String) {
    let family = exp["family"].as_str().unwrap();
    let class = exp["class"].as_str().unwrap();
    let new_id = nearest(new, family, &exp["new_at"]);
    if class == "new" {
        let Some(id) = new_id else {
            return (false, format!("no new {family} near {}", exp["new_at"]));
        };
        return (
            c.new_features.contains(&id),
            format!("{id} is not new: {:?}", c.new_features),
        );
    }
    let Some(id) = nearest(old, family, &exp["old_at"]) else {
        return (false, format!("no old {family} near {}", exp["old_at"]));
    };
    let e = c.features.iter().find(|e| e.old == id).unwrap();
    let got = format!(
        "{id}: {:?} → {:?} {:?} {:?}",
        e.class, e.new, e.candidates, e.changes
    );
    let class_ok = serde_json::to_value(e.class).unwrap() == class;
    let partner_ok = match class {
        "carried" | "adapted" => e.new.is_some() && e.new == new_id,
        "ambiguous" => new_id.is_none_or(|n| e.candidates.contains(&n)),
        _ => true,
    };
    let changes: BTreeMap<&str, ()> = exp["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| (k.as_str().unwrap(), ()))
        .collect();
    let changes_ok = changes.keys().all(|k| e.changes.contains_key(*k));
    (class_ok && partner_ok && changes_ok, got)
}
