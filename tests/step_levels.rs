//! Step levels and the aggregate's risers against Python's (`tools/capture_step_levels.py`,
//! `tests/fixtures/captured/step_levels/capture.json.gz`), part by part over the corpus:
//!
//! - `records`: [`levels::step_level_records`] at its default end margin, record by record;
//! - `proposals`, `proofs`: the step levels [`levels::discover_step_levels`] proposes, each record
//!   with its defining faces, in Python's order, and whether each proves one valid solid strictly
//!   and under local degradation ([`evidence::common_valid_solid`],
//!   [`evidence::locally_valid_solid`]);
//! - `riser_proposals`, `riser_proofs`: the same for [`levels::discover_risers`] with no body
//!   levels (Python's proposals are per solid, so both sides are compared sorted by faces);
//! - `aggregate_step_levels`, `aggregate_risers`: those proved as Python's default inventory proves
//!   them (strictly, or under local degradation where it took the retry: [`levels::proved`]),
//!   against its physical candidates;
//! - `inventory_step_levels`, `inventory_risers`, `accepted_risers`: what [`features::inventory`]
//!   carries (unproved, as every family there) and what [`features::recognise`] publishes after
//!   reconciliation, against Python's candidates and its accepted risers.
//!
//! Every difference is listed in `captured/step_levels/known_differences.json` by file and check
//! with a verdict and a reason; unlisted and stale entries fail.

mod common;

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use quiddity::Part;
use quiddity::features::evidence::{common_valid_solid, locally_valid_solid};
use quiddity::features::levels::{self, FaceLevel, RiserEvidence};
use quiddity::features::{self, Context, Occurrence};
use quiddity::kernel::step::read_step_file;
use serde_json::{Value, json};

const CAPTURE: &str = "captured/step_levels/capture.json.gz";
const KNOWN: &str = "captured/step_levels/known_differences.json";

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

fn json<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}

/// Occurrences as Python's capture writes them: each record with its defining faces, sorted.
fn entries<R: serde::Serialize>(found: &[Occurrence<R>]) -> Value {
    Value::Array(
        found
            .iter()
            .map(|o| {
                let mut defining = o.defining.clone();
                defining.sort_unstable();
                json!({"record": json(&o.record), "defining": defining})
            })
            .collect(),
    )
}

/// Python's captured entries without their proofs.
fn python_entries(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .unwrap()
            .iter()
            .map(|e| json!({"record": e["record"], "defining": e["defining"]}))
            .collect(),
    )
}

/// Entries sorted by their defining faces (Python's riser proposals are in solid order).
fn by_faces(v: Value) -> Value {
    let mut items = v.as_array().unwrap().clone();
    items.sort_by_key(|e| e["defining"].to_string());
    Value::Array(items)
}

/// Each occurrence's proofs, strictly and under local degradation.
fn proofs<R>(part: &Part, found: &[Occurrence<R>]) -> Value {
    Value::Array(
        found
            .iter()
            .map(|o| {
                let mut defining = o.defining.clone();
                defining.sort_unstable();
                json!({
                    "defining": defining,
                    "strict": common_valid_solid(part, &o.defining).is_some(),
                    "degraded": locally_valid_solid(part, &o.defining).is_some(),
                })
            })
            .collect(),
    )
}

fn python_proofs(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .unwrap()
            .iter()
            .map(|e| json!({"defining": e["defining"], "strict": e["strict"], "degraded": e["degraded"]}))
            .collect(),
    )
}

/// The aggregate's proved step levels and risers.
type Proved = (Vec<Occurrence<FaceLevel>>, Vec<Occurrence<RiserEvidence>>);

/// The checks one part fails: `(check, rust, python)`.
fn compare(dir: &Path, p: &Value) -> Vec<(&'static str, Value, Value)> {
    let file = p["file"].as_str().unwrap();
    let part = read(dir, file);
    let ctx = Context::new(&part);
    let mut out = Vec::new();
    let mut check = |name: &'static str, rust: Value, python: Value| {
        if !common::same(&rust, &python) {
            out.push((name, rust, python));
        }
    };
    check(
        "records",
        json(levels::step_level_records(&part, None)),
        p["records"].clone(),
    );
    let found = levels::discover_step_levels(&ctx);
    check(
        "proposals",
        entries(&found),
        python_entries(&p["proposals"]),
    );
    check(
        "proofs",
        proofs(&part, &found),
        python_proofs(&p["proposals"]),
    );
    let risers = levels::discover_risers(&ctx, &[]);
    check(
        "riser_proposals",
        by_faces(entries(&risers)),
        by_faces(python_entries(&p["riser_proposals"])),
    );
    check(
        "riser_proofs",
        by_faces(proofs(&part, &risers)),
        by_faces(python_proofs(&p["riser_proposals"])),
    );

    let aggregate = &p["aggregate"];
    let degraded = aggregate["retried"].as_bool().unwrap();
    let proved: Result<Proved, String> = levels::proved(&part, found, degraded)
        .and_then(|levels_found| {
            let risers = levels::discover_risers(&ctx, &levels_found);
            Ok((levels_found, levels::proved(&part, risers, degraded)?))
        })
        .map_err(|e| e.to_string());
    if let Some(error) = aggregate.get("error") {
        let rust = proved
            .err()
            .map_or(json!("published"), |e| json!({ "error": e }));
        check("aggregate", rust, json!({ "error": error }));
        return out;
    }
    // Python's own consistency: the published step levels are its candidates, and its published
    // risers its accepted candidates.
    let published: Vec<Value> = aggregate["step_levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["record"].clone())
        .collect();
    assert_eq!(
        Value::Array(published),
        aggregate["result_step_levels"],
        "{file}"
    );
    let accepted: BTreeSet<u64> = aggregate["accepted_risers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_u64().unwrap())
        .collect();
    let python_accepted: Vec<Value> = aggregate["risers"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(i, _)| accepted.contains(&(*i as u64)))
        .map(|(_, e)| e.clone())
        .collect();
    let records: Vec<Value> = python_accepted
        .iter()
        .map(|e| e["record"].clone())
        .collect();
    assert_eq!(Value::Array(records), aggregate["result_risers"], "{file}");

    match proved {
        Ok((levels_found, risers)) => {
            check(
                "aggregate_step_levels",
                entries(&levels_found),
                aggregate["step_levels"].clone(),
            );
            check(
                "aggregate_risers",
                entries(&risers),
                aggregate["risers"].clone(),
            );
        }
        Err(e) => check("aggregate", json!({ "error": e }), json!("published")),
    }

    let inventory = features::inventory(&part).unwrap_or_else(|e| panic!("{file}: {e}"));
    let carried = |f: &features::Features, family: &str| -> Value {
        let records = json(f)[family].clone();
        Value::Array(
            records
                .as_array()
                .unwrap()
                .iter()
                .zip(&f.defining[family])
                .map(|(record, defining)| {
                    let mut defining = defining.clone();
                    defining.sort_unstable();
                    json!({"record": record, "defining": defining})
                })
                .collect(),
        )
    };
    check(
        "inventory_step_levels",
        carried(&inventory.physical, "step_levels"),
        aggregate["step_levels"].clone(),
    );
    check(
        "inventory_risers",
        carried(&inventory.physical, "risers"),
        aggregate["risers"].clone(),
    );
    check(
        "accepted_risers",
        carried(&inventory.accepted(), "risers"),
        Value::Array(python_accepted),
    );
    out
}

#[test]
fn step_levels_and_risers_agree_with_python() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let capture = capture();
    let entries = parts(&capture);
    let found = common::parallel::map(entries, |p| compare(&dir, p));
    let count = |key: &str| -> usize {
        entries
            .iter()
            .map(|p| p["aggregate"][key].as_array().map_or(0, Vec::len))
            .sum()
    };
    let accepted = count("accepted_risers");
    eprintln!(
        "step levels: {} public records, {} aggregate step levels, {} aggregate risers ({} \
         accepted, {} rejected) over {} parts",
        entries
            .iter()
            .map(|p| p["records"].as_array().unwrap().len())
            .sum::<usize>(),
        count("step_levels"),
        count("risers"),
        accepted,
        count("risers") - accepted,
        entries.len()
    );
    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts(KNOWN, known);
    let key = |e: &Value| {
        (
            e["file"].as_str().unwrap().to_owned(),
            e["check"].as_str().unwrap().to_owned(),
        )
    };
    let listed: Vec<(String, String)> = known.iter().map(key).collect();
    let mut seen = Vec::new();
    let mut problems = Vec::new();
    for (p, differences) in entries.iter().zip(found) {
        let file = p["file"].as_str().unwrap();
        for (check, rust, python) in differences {
            let at = (file.to_owned(), check.to_owned());
            if listed.contains(&at) {
                seen.push(at);
            } else {
                problems.push(format!("{file} {check}: {}", common::diff(&rust, &python)));
            }
        }
    }
    for at in &listed {
        if !seen.contains(at) {
            problems.push(format!("{at:?} is listed but now agrees: remove it"));
        }
    }
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), listed.len(), "an entry is listed twice");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
