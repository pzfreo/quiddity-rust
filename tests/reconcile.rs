//! The aggregate's cross-family reconciliation decisions against Python's
//! (`tools/capture_reconcile.py`, `tests/fixtures/captured/reconcile/capture.json.gz`).
//!
//! Python's default inventory (`_take_inventory`, local-degradation retry included) decides
//! between families in `_reconcile_existing`; the port's [`reconcile::reconcile`] makes the same
//! decisions on [`features::inventory`]'s candidates. On every corpus part the two are compared as
//! sets keyed by family and defining faces: each decision's outcome, reason and related
//! candidates (by their faces). Python's default acceptances are not compared (whether a record
//! exists at all is `tests/corpus.rs`'s question). The records `features::recognise` keeps (the
//! accepted inventory) are compared too, family by family among those a rule reads, with
//! Python's candidates less its rejected ones; differences are listed under `accepted` as
//! `{"file", "family", "python", "rust"}` (the defining-face lists only that side has) or
//! `{"file", "family", "order": true}`.
//!
//! Every difference is listed under `decisions` in `captured/reconcile/known.json` with a
//! verdict and a reason: `{"file", "family", "faces", "python", "rust"}`, where `python` and
//! `rust` are each side's decision on that candidate (`null` for none). Unlisted, stale and
//! doubly listed differences fail, and a failing run prints the entries it needs. The port's
//! decisions are also compared with themselves under two rigid motions (`motions` there lists
//! those that move, `rust` unmoved and `moved`), and Python's on synthetic scenarios reaching the
//! branches the corpus does not (`captured/reconcile/scenarios.json`), which must all agree.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use quiddity::Part;
use quiddity::features;
use quiddity::features::passage_compat::PassageCompatibilityView;
use quiddity::features::reconcile::{self, Disposition, Evidence};
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const CAPTURE: &str = "captured/reconcile/capture.json.gz";
const KNOWN: &str = "captured/reconcile/known.json";

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

fn capture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(common::fixtures().join(CAPTURE)).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn parts(capture: &Value) -> &Vec<Value> {
    capture["parts"].as_array().unwrap()
}

/// One side's decisions by (family, defining faces): outcome, reason and related faces (sorted;
/// the related family is the reason's).
type Decided = BTreeMap<(String, Vec<usize>), Vec<Value>>;

fn summary(outcome: &str, reason: &str, mut related: Vec<Vec<usize>>) -> Value {
    related.sort();
    json!({"outcome": outcome, "reason": reason, "related": related})
}

fn faces(v: &Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_u64().unwrap() as usize)
        .collect()
}

fn python_decisions(part: &Value) -> Result<Decided, String> {
    if let Some(error) = part.get("error") {
        return Err(error.as_str().unwrap().to_owned());
    }
    let mut out = Decided::new();
    for d in part["decisions"].as_array().unwrap() {
        let candidate = d["candidate"].as_array().unwrap();
        let related = d["related"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| faces(&r[2]))
            .collect();
        out.entry((
            candidate[0].as_str().unwrap().to_owned(),
            faces(&candidate[2]),
        ))
        .or_default()
        .push(summary(
            d["outcome"].as_str().unwrap(),
            d["reason"].as_str().unwrap(),
            related,
        ));
    }
    Ok(out)
}

/// The port's decisions on *part*, or its refusal.
fn port_decisions(part: &Part) -> Result<Decided, String> {
    decided(features::inventory(part).map(|i| i.dispositions))
}

fn decided(
    decisions: Result<Vec<Disposition>, reconcile::ReconcileError>,
) -> Result<Decided, String> {
    let decisions = decisions.map_err(|e| e.to_string())?;
    let mut out = Decided::new();
    for d in decisions {
        out.entry((d.candidate.family.value().to_owned(), d.candidate.defining))
            .or_default()
            .push(summary(
                d.outcome.value(),
                d.reason.value(),
                d.related.into_iter().map(|r| r.defining).collect(),
            ));
    }
    Ok(out)
}

/// Each candidate either side decided differently, as an entry without its verdict.
fn differences(
    file: &str,
    python: &Result<Decided, String>,
    rust: &Result<Decided, String>,
) -> Vec<Value> {
    let (python, rust) = match (python, rust) {
        (Ok(p), Ok(r)) => (p, r),
        _ => {
            let side = |s: &Result<Decided, String>| match s {
                Ok(_) => json!("decided"),
                Err(e) => json!({"error": e}),
            };
            if python.as_ref().err() == rust.as_ref().err() {
                return Vec::new();
            }
            return vec![json!({"file": file, "family": null, "faces": null,
                               "python": side(python), "rust": side(rust)})];
        }
    };
    let mut keys: Vec<&(String, Vec<usize>)> = python.keys().chain(rust.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|key| {
            let (p, r) = (python.get(key), rust.get(key));
            let sorted = |v: Option<&Vec<Value>>| {
                v.map(|v| {
                    let mut v = v.clone();
                    v.sort_by_key(|s| s.to_string());
                    if v.len() == 1 {
                        v.remove(0)
                    } else {
                        Value::Array(v)
                    }
                })
            };
            let (p, r) = (sorted(p), sorted(r));
            (p != r).then(
                || json!({"file": file, "family": key.0, "faces": key.1, "python": p, "rust": r}),
            )
        })
        .collect()
}

/// Every difference must be listed once, and every listed one found.
fn check_known(found: Vec<Value>, list: &str) {
    let known = common::load(KNOWN);
    let entries = known[list].as_array().unwrap();
    common::check_verdicts(&format!("reconcile/known.json {list}"), entries);
    let key = |e: &Value| {
        let mut e = e.clone();
        let map = e.as_object_mut().unwrap();
        map.remove("verdict");
        map.remove("reason");
        e.to_string()
    };
    let mut listed: BTreeMap<String, bool> = BTreeMap::new();
    let mut problems = Vec::new();
    for e in entries {
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

/// The capture is of this corpus, at the revision `corpus.json` records (every file and its
/// sha256).
#[test]
fn capture_covers_the_corpus() {
    let capture = capture();
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
}

fn read(dir: &Path, file: &str) -> Part {
    read_step_file(&dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn decisions_agree_with_python() {
    let Some(dir) = corpus() else { return };
    let capture = capture();
    let entries = parts(&capture);
    let found = common::parallel::map(entries, |p| {
        let file = p["file"].as_str().unwrap();
        let inventory = features::inventory(&read(&dir, file));
        let rust = decided(inventory.clone().map(|i| i.dispositions));
        let python = python_decisions(p);
        let count =
            |d: &Result<Decided, String>| d.as_ref().map_or(0, |d| d.values().map(Vec::len).sum());
        (
            count(&python),
            count(&rust),
            differences(file, &python, &rust),
            inventory.map_or_else(
                |_| Vec::new(),
                |i| accepted_differences(file, p, i.accepted()),
            ),
        )
    });
    let (python, rust): (usize, usize) = found.iter().fold((0, 0), |(p, r), f| (p + f.0, r + f.1));
    let (differences, accepted): (Vec<Vec<Value>>, Vec<Vec<Value>>) =
        found.into_iter().map(|f| (f.2, f.3)).unzip();
    let differences: Vec<Value> = differences.into_iter().flatten().collect();
    eprintln!(
        "reconcile: {python} Python decisions, {rust} port decisions over {} parts; {} differ",
        entries.len(),
        differences.len()
    );
    check_known(differences, "decisions");
    check_known(accepted.into_iter().flatten().collect(), "accepted");
}

/// Where `features::recognise`'s records (the accepted inventory) differ from Python's accepted
/// candidates (the captured candidates less every rejected one), family by family among those a
/// rule reads: the defining-face lists only one side has, or `order` where both have the same
/// lists in another order. Risers are not carried.
fn accepted_differences(file: &str, part: &Value, accepted: features::Features) -> Vec<Value> {
    let rejected: Vec<(&str, usize)> = part["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["outcome"] == "rejected")
        .map(|d| {
            let c = d["candidate"].as_array().unwrap();
            (c[0].as_str().unwrap(), c[1].as_u64().unwrap() as usize)
        })
        .collect();
    let mut out = Vec::new();
    for (family, field) in FIELDS {
        let python: Vec<Vec<usize>> = face_lists(part["candidates"].get(family))
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !rejected.contains(&(family, *i)))
            .map(|(_, f)| f)
            .collect();
        let rust: Vec<Vec<usize>> = accepted.defining[field]
            .iter()
            .map(|f| {
                let mut f = f.clone();
                f.sort_unstable();
                f
            })
            .collect();
        if python == rust {
            continue;
        }
        let only = |a: &[Vec<usize>], b: &[Vec<usize>]| {
            let mut rest = b.to_vec();
            let mut out = Vec::new();
            for f in a {
                match rest.iter().position(|g| g == f) {
                    Some(at) => {
                        rest.remove(at);
                    }
                    None => out.push(f.clone()),
                }
            }
            out
        };
        let (p, r) = (only(&python, &rust), only(&rust, &python));
        out.push(if p.is_empty() && r.is_empty() {
            json!({"file": file, "family": family, "order": true})
        } else {
            json!({"file": file, "family": family, "python": p, "rust": r})
        });
    }
    out
}

/// The `Features` fields (`Defining` keys) the rules read, by Python family value.
const FIELDS: [(&str, &str); 19] = [
    ("angled_steps", "angled_steps"),
    ("blends", "blends"),
    ("bosses", "bosses"),
    ("chamfers", "chamfers"),
    ("circular_blind_steps", "circular_blind_steps"),
    ("double_d_bores", "double_d_bores"),
    ("edge_open_circular_pockets", "edge_open_circular_pockets"),
    ("fillets", "fillets"),
    ("grooves", "grooves"),
    ("holes", "holes"),
    ("oriented_slots", "oriented_slots"),
    ("passages", "section_passages"),
    ("plates", "plates"),
    ("pockets", "pockets"),
    ("prismatic_pockets", "prismatic_pockets"),
    ("rectangular_blind_slots", "rectangular_blind_slots"),
    ("slots", "slots"),
    ("thin_wall_bodies", "thin_wall_bodies"),
    ("turned_steps", "turned_steps"),
];

fn face_lists(v: Option<&Value>) -> Vec<Vec<usize>> {
    v.map_or_else(Vec::new, |v| {
        v.as_array().unwrap().iter().map(faces).collect()
    })
}

/// Python's decisions on the synthetic scenarios that reach the branches the corpus does not
/// (`tools/capture_reconcile.py --scenarios`), rebuilt from the same inputs: every one agrees.
#[test]
fn scenarios_agree_with_python() {
    let captured = common::load("captured/reconcile/scenarios.json");
    let scenarios = captured["scenarios"].as_array().unwrap();
    for s in scenarios {
        let families = &s["families"];
        for name in families.as_object().unwrap().keys() {
            assert!(
                name == "risers" || FIELDS.iter().any(|(f, _)| f == name),
                "{name}: no field"
            );
        }
        let defining: features::Defining = FIELDS
            .iter()
            .map(|&(family, field)| (field, face_lists(families.get(family))))
            .collect();
        let sides: Vec<usize> = s.get("prismatic_sides").map_or_else(Vec::new, |v| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect()
        });
        let groupings = s
            .get("passage_groupings")
            .map_or_else(Vec::new, |v| v.as_array().unwrap().clone());
        let view = |g: &Value| {
            if g.is_null() {
                return PassageCompatibilityView::new(None, None, None, None, None, None, false);
            }
            let axis = ["x", "y", "z"]
                .into_iter()
                .find(|a| *a == g[0])
                .expect("an axis");
            let section = g[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
                .collect();
            let sides = g[2].as_u64().map(|n| n as usize);
            PassageCompatibilityView::new(Some(axis), Some(section), sides, None, None, None, false)
        };
        let evidence = Evidence {
            pocket_constituent: face_lists(s.get("pocket_constituent")),
            passage_compatibility: groupings.iter().map(|g| view(g).unwrap()).collect(),
            risers: face_lists(families.get("risers")),
        };
        let rust = decided(reconcile::reconcile_faces(&defining, &sides, &evidence));
        let python = python_decisions(s);
        let name = s["name"].as_str().unwrap();
        assert_eq!(differences(name, &python, &rust), Vec::<Value>::new());
    }
    // Every rule's reason is reached, here or on the corpus.
    let reasons: Vec<&str> = scenarios
        .iter()
        .flat_map(|s| s["decisions"].as_array().into_iter().flatten())
        .map(|d| d["reason"].as_str().unwrap())
        .collect();
    let corpus = capture();
    let mut missing: Vec<&str> = reconcile::ReasonCode::ALL
        .iter()
        .map(|r| r.value())
        .filter(|r| !reasons.contains(r))
        .collect();
    missing.retain(|r| {
        !parts(&corpus).iter().any(|p| {
            p["decisions"]
                .as_array()
                .is_some_and(|d| d.iter().any(|d| d["reason"] == *r))
        })
    });
    assert!(missing.is_empty(), "reasons never exercised: {missing:?}");
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

/// The port's decisions are the same faces however the part is placed.
#[test]
fn decisions_are_placement_independent() {
    let Some(dir) = corpus() else { return };
    let capture = capture();
    let found = common::parallel::map(parts(&capture), |p| {
        let file = p["file"].as_str().unwrap();
        let path = dir.join(file);
        let want = port_decisions(&read(&dir, file));
        let mut out = Vec::new();
        for (motion, placement) in &MOTIONS {
            let moved = read_step_file_placed(&path, placement).unwrap();
            let got = port_decisions(&moved);
            for mut d in differences(file, &want, &got) {
                let map = d.as_object_mut().unwrap();
                map.insert("motion".into(), json!(motion));
                map.insert("moved".into(), map["rust"].clone());
                map.insert("rust".into(), map["python"].clone());
                map.remove("python");
                out.push(d);
            }
        }
        out
    });
    check_known(found.into_iter().flatten().collect(), "motions");
}
