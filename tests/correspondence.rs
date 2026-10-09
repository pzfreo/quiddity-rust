//! Revision correspondence (`docs/correspondence.md`, "Tests").
//!
//! - Invariance: every corpus part, read under each of `tests/invariance.rs`'s rigid motions,
//!   corresponds with its unmoved self with every feature and every face carried, each to the
//!   very feature (same defining faces) and face (same index) it is.
//! - Revision pairs: build123d parts revised as a designer would (`tools/capture_revisions.py`,
//!   `tests/fixtures/revisions/`), each feature's class checked against the expected one, each
//!   carried feature's faces (patterns aside) checked carried onto its partner's, and every face of a whole-part
//!   motion checked carried.
//!
//! Differences are listed with verdicts in `tests/fixtures/known_correspondence.json`; the test
//! fails on an unlisted one and on a listed one that has gone.

mod common;
#[macro_use]
#[path = "support/slices.rs"]
mod slices;

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

/// The families caller-space recognition reads along world Z (`quiddity.levels`).
const WORLD_Z: [&str; 2] = ["risers", "step_levels"];

fn placement(r: &[[f64; 3]; 3], t: &[f64; 3]) -> Placement {
    [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]])
}

fn faces_of<'a>(f: &'a Fingerprints, id: &str) -> &'a [usize] {
    &f.features.iter().find(|x| x.id == id).unwrap().faces
}

/// What a correspondence of a part with itself moved gets wrong: the old features not carried
/// to themselves (and new features, as `new:<id>`), the faces not carried to themselves, and a
/// description. Families whose recognition is listed as not invariant (`known_invariance.json`)
/// are left out, with the patterns made of them.
struct Problems {
    features: Vec<String>,
    faces: usize,
    text: Vec<String>,
}

fn invariance_problems(
    old: &Fingerprints,
    new: &Fingerprints,
    c: &Correspondence,
    skip: &[String],
) -> Problems {
    let skipped = |id: &str| {
        let family = id.split('/').next().unwrap();
        skip.iter().any(|s| {
            family == s
                || (s == "holes" && family == "hole_patterns")
                || (s == "slots" && family == "slot_patterns")
                || (s == "pockets" && family == "pocket_patterns")
                || (s == "oriented_slots" && family == "oriented_slot_patterns")
        })
    };
    let mut out = Problems {
        features: Vec::new(),
        faces: 0,
        text: Vec::new(),
    };
    for e in &c.features {
        if skipped(&e.old) {
            continue;
        }
        let ok = e.class == Class::Carried
            && e.new
                .as_deref()
                .is_some_and(|n| faces_of(new, n) == faces_of(old, &e.old));
        if !ok {
            out.features.push(e.old.clone());
            out.text.push(format!(
                "feature {} {:?} → {:?} {:?} {:?}",
                e.old, e.class, e.new, e.candidates, e.changes
            ));
        }
    }
    for id in &c.new_features {
        if !skipped(id) {
            out.features.push(format!("new:{id}"));
            out.text.push(format!("feature {id} new"));
        }
    }
    let faces: Vec<String> = c
        .faces
        .iter()
        .filter(|e| !(e.class == Class::Carried && e.new == Some(e.old)))
        .map(|e| format!("face {} {:?} → {:?} {:?}", e.old, e.class, e.new, e.changes))
        .collect();
    out.faces = faces.len();
    if !faces.is_empty() {
        out.text.push(format!(
            "{} faces not carried to themselves, e.g. {}",
            faces.len(),
            faces[0]
        ));
    }
    out
}

/// A listed invariance case: (file, motion), and the features and faces that differ.
type Case = (String, String);
type Listed = (Vec<String>, Faces);

/// How many faces a listed case leaves not carried to themselves. `faces` pins the count exactly
/// (the default). `faces_at_most` bounds it, for a case whose count depends on a float threshold:
/// a face is not carried when its kernel area moves with the placement by more than the freeform
/// tolerance, and whether a wrong area lands just over or just under it depends on the platform's
/// libm (cgb207 under `rot_zx_moved`: 2 faces on macOS, 1 on Linux). The features stay pinned
/// exactly either way. Linux (CI) is the reference platform for these pins: a bound is at least
/// the count CI gives, and no lower than the count seen on any other platform.
#[derive(Clone, Copy, PartialEq)]
enum Faces {
    Exactly(usize),
    AtMost(usize),
}

impl Faces {
    fn read(k: &Value) -> Option<Faces> {
        match (k["faces"].as_u64(), k["faces_at_most"].as_u64()) {
            (Some(n), None) => Some(Faces::Exactly(n as usize)),
            (None, Some(n)) => Some(Faces::AtMost(n as usize)),
            _ => None,
        }
    }

    fn admits(self, n: usize) -> bool {
        match self {
            Faces::Exactly(m) => n == m,
            Faces::AtMost(m) => n <= m,
        }
    }
}

#[test]
fn listed_face_counts_are_pinned_or_bounded() {
    let exact = Faces::read(&serde_json::json!({"faces": 2})).unwrap();
    let bound = Faces::read(&serde_json::json!({"faces_at_most": 2})).unwrap();
    assert!(exact.admits(2) && !exact.admits(1) && !exact.admits(3));
    assert!(bound.admits(2) && bound.admits(1) && !bound.admits(3));
    assert!(Faces::read(&serde_json::json!({"faces": 2, "faces_at_most": 2})).is_none());
    assert!(Faces::read(&serde_json::json!({})).is_none());
}

impl std::fmt::Display for Faces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Faces::Exactly(n) => write!(f, "{n} faces"),
            Faces::AtMost(n) => write!(f, "at most {n} faces"),
        }
    }
}

/// One part moved: what its correspondence with the unmoved part gets wrong, its alignment, and
/// how much there was to carry.
struct Moved {
    found: Problems,
    alignment: String,
    features: usize,
    faces: usize,
}

fn known() -> Vec<Value> {
    let known = common::load("known_correspondence.json");
    let known = known.as_array().unwrap().clone();
    common::check_verdicts("known_correspondence.json", &known);
    known
}

fn corpus_files() -> Vec<String> {
    common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_string())
        .collect()
}

/// Every corpus part under every motion: one test per slice of the corpus
/// (`tests/support/slices.rs`), each checking the listed cases of its files; the totals printed
/// are the slice's.
fn correspond_with_themselves_moved(slice: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let all = corpus_files();
    // Each listed case pins exactly which features and how many faces differ (or, for a
    // threshold-dependent count, at most how many), so any other difference in it still fails.
    let known: Vec<(Case, Listed)> = known()
        .iter()
        .filter(|e| {
            e["test"] == "invariance" && slices::owns(&all, slice, n, e["file"].as_str().unwrap())
        })
        .map(|k| {
            let mut features: Vec<String> = k["features"]
                .as_array()
                .expect("an invariance entry lists its features")
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect();
            features.sort();
            (
                (
                    k["file"].as_str().unwrap().to_string(),
                    k["motion"].as_str().unwrap().to_string(),
                ),
                (
                    features,
                    Faces::read(k).unwrap_or_else(|| {
                        panic!("an invariance entry gives one of faces and faces_at_most: {k}")
                    }),
                ),
            )
        })
        .collect();
    let invariance = common::load("known_invariance.json");
    let files = slices::slice(&all, slice, n);
    // Every part unmoved and under every motion, all spread over every core at once (so a costly
    // part's readings run side by side), then each moved part corresponded with its unmoved self;
    // the results come back in corpus order, motion by motion.
    let readings: Vec<(usize, Option<&Motion>)> = (0..files.len())
        .flat_map(|i| {
            std::iter::once(None)
                .chain(MOTIONS.iter().map(Some))
                .map(move |m| (i, m))
        })
        .collect();
    let mut read = common::parallel::map(&readings, |&(i, motion)| {
        let at = motion.map_or(IDENTITY, |(_, r, t)| placement(r, t));
        read_step_file_placed(&dir.join(&files[i]), &at)
            .map(|p| correspondence::recognise(&p).fingerprints)
            // (The error as text: the kernel's error is not `Send`.)
            .map_err(|e| e.to_string())
    })
    .into_iter();
    let (mut unmoved, mut prints) = (Vec::new(), Vec::new());
    for _ in &files {
        unmoved.push(read.next().unwrap().ok());
        prints.push(read.by_ref().take(MOTIONS.len()).collect::<Vec<_>>());
    }
    let cases: Vec<(usize, usize, &Motion)> = (0..files.len())
        .filter(|&i| unmoved[i].is_some()) // the corpus test reports read failures
        .flat_map(|i| {
            MOTIONS
                .iter()
                .enumerate()
                .map(move |(m, motion)| (i, m, motion))
        })
        .collect();
    let moved = common::parallel::map(&cases, |&(i, m, (motion, r, _))| {
        let name = &files[i];
        let old = unmoved[i].as_ref().unwrap();
        let new = prints[i][m].as_ref().unwrap();
        let c = correspondence::correspond(old, new).unwrap();
        let mut skip: Vec<String> = invariance
            .as_array()
            .unwrap()
            .iter()
            .filter(|k| k["file"] == name.as_str() && k["motion"] == *motion)
            .map(|k| k["family"].as_str().unwrap().to_string())
            .collect();
        // Read along world Z by specification (`tests/invariance.rs`' WORLD_Z): where the motion
        // does not keep Z vertical, the moved part's levels and risers are other faces.
        if r[2][2].abs() != 1.0 {
            skip.extend(WORLD_Z.map(String::from));
        }
        Moved {
            found: invariance_problems(old, new, &c, &skip),
            alignment: format!(
                "aligned {}, symmetric {}, fold {:?}",
                c.alignment.found, c.alignment.symmetric, c.alignment.fold
            ),
            features: old.features.len(),
            faces: old.faces.len(),
        }
    });
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    let (mut pairs, mut features, mut faces) = (0, 0, 0);
    for (&(i, _, (motion, _, _)), m) in cases.iter().zip(moved) {
        let name = &files[i];
        let found = m.found;
        pairs += 1;
        features += m.features;
        faces += m.faces;
        let key = (name.to_string(), motion.to_string());
        let mut got = found.features.clone();
        got.sort();
        if let Some((_, listed)) = known.iter().find(|k| k.0 == key) {
            if found.text.is_empty() {
                problems.push(format!("{key:?} is listed but now carried: remove it"));
            } else if got != listed.0 || !listed.1.admits(found.faces) {
                problems.push(format!(
                    "{key:?} is listed with features {:?} and {}, but now {got:?} and {} faces: {}",
                    listed.0,
                    listed.1,
                    found.faces,
                    found.text.join("; ")
                ));
            }
            seen.push(key);
            continue;
        }
        if !found.text.is_empty() {
            problems.push(format!(
                "{name} {motion} ({}): {} | features {got:?}, faces {}",
                m.alignment,
                found.text.join("; "),
                found.faces
            ));
        }
    }
    for (key, _) in &known {
        if !seen.contains(key) {
            problems.push(format!("{key:?} is listed but was not checked"));
        }
    }
    eprintln!("{pairs} moved parts: {features} features, {faces} faces");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    corpus_parts_correspond_with_themselves_moved,
    super::correspond_with_themselves_moved,
    [
        0 => slice_0,
        1 => slice_1,
        2 => slice_2,
        3 => slice_3,
        4 => slice_4,
        5 => slice_5,
        6 => slice_6,
        7 => slice_7,
    ]
);

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
        // Faces: each carried feature's faces carried onto its partner's faces, and with nothing
        // expected to change (a whole-part motion), every face carried and none new.
        // (A pattern's faces are its members', which are checked as features in their own right.)
        for e in &c.features {
            if e.class != Class::Carried || e.old.split('/').next().unwrap().ends_with("_patterns")
            {
                continue;
            }
            let theirs = faces_of(&new, e.new.as_deref().unwrap());
            for &f in faces_of(&old, &e.old) {
                let fe = &c.faces[f];
                results.push((
                    format!("faces of {}: face {f} carried", e.old),
                    fe.class == Class::Carried && fe.new.is_some_and(|n| theirs.contains(&n)),
                    format!("{:?} → {:?} {:?}", fe.class, fe.new, fe.changes),
                ));
            }
        }
        if case["expect"].as_array().unwrap().is_empty() {
            let moved: Vec<String> = c
                .faces
                .iter()
                .filter(|e| e.class != Class::Carried)
                .map(|e| format!("{} {:?}", e.old, e.class))
                .collect();
            results.push((
                "every face carried".into(),
                moved.is_empty() && c.new_faces.is_empty(),
                format!("{moved:?}, new {:?}", c.new_faces),
            ));
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
