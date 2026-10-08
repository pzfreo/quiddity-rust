//! Passages against Python's answers, from `tests/fixtures/captured/passages/`:
//!
//! - `calls.json`: every `recognise_passages` and `recognise_section_passages` call the Python
//!   tests make (`tools/capture_plugin.py`), replayed on the part's STEP beside it;
//! - `helpers.json.gz` (`tools/capture_passages.py`): on those parts, the section helpers' test
//!   parts, the golden fixtures and the corpus, every `_rings.rings` ring (walls, section, axis,
//!   span, caps), the legacy roster's walls, the section passages with their defining walls (or
//!   the refusal), and every `principal_projection` and `_same_legacy_passage_geometry` call
//!   made on the way, each replayed on its own.
//!
//! Floats agree to one part in a million (`common::same`), refusals exactly. Differences are
//! listed in `captured/passages/known_divergences.json`: per entry its `case` (`call`, `rings`,
//! `roster`, `records`, `defining`, `projection`, `same_legacy`) and what identifies it (a call's
//! `function`, `test`, `file` and `options`; otherwise the part's `file`), a verdict and a
//! reason. A failing run prints the entries it needs.

mod common;

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

use quiddity::Part;
use quiddity::features::Context;
use quiddity::features::passage_compat::principal_projection;
use quiddity::features::passages::{Passage, legacy_roster, same_legacy_passage_geometry};
use quiddity::features::rings::rings;
use quiddity::kernel::step::read_step_file;
use serde_json::{Value, json};

/// Calls the Python tests make that the capture could not record (`skipped` in `calls.json`,
/// parts STEP export refuses), per entry point: pinned, as they are outside the replay.
const SKIPPED: &[(&str, usize)] = &[("recognise_passages", 2), ("recognise_section_passages", 3)];

fn dir() -> PathBuf {
    common::fixtures().join("captured/passages")
}

fn load_gz(name: &str) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(dir().join(name)).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn v3(v: &Value) -> [f64; 3] {
    floats(v).try_into().unwrap()
}

fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| floats(p).try_into().unwrap())
        .collect()
}

fn passage(v: &Value) -> Option<Passage> {
    (!v.is_null()).then(|| Passage {
        axis: v["axis"].as_str().unwrap().into(),
        sides: v["sides"].as_u64().unwrap() as usize,
        length: v["length"].as_f64().unwrap(),
        at: v3(&v["at"]),
        section: points(&v["section"]),
    })
}

/// Checks *found* differences (identity, description) against the listed entries, failing on
/// an unlisted difference, one listed twice, or a listed entry that no longer differs (among
/// the identities *checked*).
fn check_known(found: Vec<(Value, String)>, checked: impl Fn(&Value) -> bool) {
    let known = serde_json::from_str::<Value>(
        &std::fs::read_to_string(dir().join("known_divergences.json")).unwrap(),
    )
    .unwrap();
    let known = known.as_array().unwrap();
    common::check_verdicts("captured/passages/known_divergences.json", known);
    let listed: Vec<&Value> = known.iter().filter(|k| checked(k)).collect();
    let mut problems = Vec::new();
    let mut seen = vec![false; listed.len()];
    for (identity, text) in &found {
        let position = listed.iter().position(|k| {
            identity
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| &k[key] == value)
        });
        match position {
            Some(i) if seen[i] => problems.push(format!("listed twice: {identity}")),
            Some(i) => seen[i] = true,
            None => problems.push(format!("unlisted difference: {text}\n  entry: {identity}")),
        }
    }
    for (k, seen) in listed.iter().zip(seen) {
        if !seen {
            problems.push(format!("listed but no longer differs: {k}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn passage_calls_match_python() {
    let calls: Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("calls.json")).unwrap()).unwrap();
    for (function, pinned) in SKIPPED {
        let skipped = calls["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["function"] == *function)
            .count();
        assert_eq!(
            skipped, *pinned,
            "{function}: calls the capture could not record"
        );
    }
    let calls = calls["calls"].as_array().unwrap();
    let mut parts: std::collections::BTreeMap<String, Part> = Default::default();
    let mut found = Vec::new();
    let mut matched = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for c in calls {
        let function = c["function"].as_str().unwrap();
        let file = c["file"].as_str().expect("a call on a part");
        let part = parts
            .entry(file.to_owned())
            .or_insert_with(|| read_step_file(&dir().join(file)).unwrap());
        let got = common::recognise(function, part, &c["options"]);
        let tally = matched.entry(function).or_default();
        if common::same(&got, &c["result"]) {
            tally.0 += 1;
        } else {
            tally.1 += 1;
            found.push((
                json!({"case": "call", "function": function, "test": c["test"], "file": file,
                       "options": c["options"]}),
                format!("rust   {got}\n  python {}", c["result"]),
            ));
        }
    }
    for (function, (ok, bad)) in &matched {
        eprintln!("{function}: {ok} match, {bad} differ");
    }
    check_known(found, |k| k["case"] == "call");
}

/// The port's answer to a captured helper call.
fn helper_answer(call: &Value) -> Value {
    let given = call["given"].as_array().unwrap();
    match call["kind"].as_str().unwrap() {
        "principal_projection" => {
            let interval = floats(&given[4]);
            match principal_projection(
                v3(&given[0]),
                v3(&given[1]),
                v3(&given[2]),
                v3(&given[3]),
                (interval[0], interval[1]),
                &points(&given[5]),
            ) {
                Some(p) => json!([p.axis, p.sides, p.length, p.at, p.section]),
                None => Value::Null,
            }
        }
        "same_legacy_passage_geometry" => json!(same_legacy_passage_geometry(
            passage(&given[0]).as_ref(),
            &passage(&given[1]).unwrap(),
            (!given[2].is_null()).then(|| v3(&given[2])),
        )),
        other => panic!("unknown helper call {other}"),
    }
}

fn check_part(run: &Value, part: &Part) -> Vec<(Value, String)> {
    let file = run["file"].as_str().unwrap();
    let ctx = Context::new(part);
    let mut found = Vec::new();
    let mut differ = |case: &str, got: Value, want: &Value| {
        if !common::same(&got, want) {
            found.push((
                json!({"case": case, "file": file}),
                format!("{file} {case}\n  rust   {got}\n  python {want}"),
            ));
        }
    };
    // Rings in a canonical order: Python's component order is unspecified.
    let mut got: Vec<Value> = rings(&ctx)
        .into_iter()
        .map(|r| {
            json!({"nodes": r.nodes, "section": r.section, "axis": r.axis, "low": r.low,
                   "high": r.high, "caps": [r.cap_nodes.0, r.cap_nodes.1]})
        })
        .collect();
    let mut want = run["rings"].as_array().unwrap().clone();
    let key = |r: &Value| (r["axis"].as_u64(), r["nodes"].to_string());
    got.sort_by_key(key);
    want.sort_by_key(key);
    differ("rings", json!(got), &json!(want));
    let roster: Vec<Vec<usize>> = legacy_roster(&ctx)
        .into_iter()
        .map(|(_, mut nodes)| {
            nodes.sort_unstable();
            nodes
        })
        .collect();
    differ("roster", json!(roster), &run["roster"]);
    let records = match quiddity::recognise_section_passages(part) {
        Ok(found) => json!({"result": found}),
        Err(e) => json!({"refused": e.to_string()}),
    };
    let want = match run.get("refused") {
        Some(r) => json!({"refused": r}),
        None => json!({"result": run["result"]}),
    };
    differ("records", records, &want);
    if let Some(defining) = run.get("defining") {
        let got = quiddity::features::passages::discover_verified(&ctx)
            .map(|found| found.into_iter().map(|o| o.defining).collect::<Vec<_>>());
        differ("defining", json!(got.ok()), defining);
    }
    for call in run["calls"].as_array().unwrap() {
        let case = match call["kind"].as_str().unwrap() {
            "principal_projection" => "projection",
            _ => "same_legacy",
        };
        differ(case, helper_answer(call), &call["answer"]);
    }
    found
}

#[test]
fn passage_helpers_match_python() {
    let helpers = load_gz("helpers.json.gz");
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; its parts are skipped");
    }
    let runs: Vec<&Value> = helpers["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["source"] != "corpus" || corpus.is_some())
        .collect();
    let ran: BTreeSet<String> = runs
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect();
    let found: Vec<(Value, String)> = common::parallel::map(&runs, |run| {
        let file = run["file"].as_str().unwrap();
        let path = match run["source"].as_str().unwrap() {
            "test" => common::fixtures().join("captured").join(file),
            "fixture" => common::fixtures().join(file),
            _ => corpus.as_ref().unwrap().join(file),
        };
        check_part(run, &read_step_file(&path).unwrap())
    })
    .into_iter()
    .flatten()
    .collect();
    let counts = |case: &str| found.iter().filter(|(i, _)| i["case"] == case).count();
    eprintln!(
        "{} parts; differing: rings {}, roster {}, records {}, defining {}, projection {}, \
         same_legacy {}",
        runs.len(),
        counts("rings"),
        counts("roster"),
        counts("records"),
        counts("defining"),
        counts("projection"),
        counts("same_legacy"),
    );
    check_known(found, |k| {
        k["case"] != "call" && k["file"].as_str().is_some_and(|f| ran.contains(f))
    });
}
