//! The section-recess family's entry point against Python's (`tools/capture_section_recesses.py`,
//! fixtures in `tests/fixtures/captured/section_recesses/`): `build_section_recess_document` on
//! the parts the section-recess Python tests hand `build_section_recess_document` or
//! `recognise_section_recesses`, the two golden passage fixtures and every corpus part.
//!
//! The port's document is compared with Python's item by item: occurrences by body and
//! constituent faces (the region `_unique_section_recesses` keys them by), refusals by body and
//! evidence, patterns by their members' regions; floats agree to one part in a million
//! (`common::same`), and the occurrences' order (their indices) must agree too. A document
//! Python refuses must be refused with the same message. A part whose projection Python did not
//! finish within the capture's timeout is not compared, and is counted.
//!
//! Differences are listed in `known_differences.json` beside the fixtures: per entry the part's
//! `file`, the `item` (`document`, `order`, `occurrence`, `refusal`, `pattern`), its `key` (the
//! region, or for a pattern its members' regions) and `kind` (`missing`: Python's only,
//! `extra`: the port's only, `differs`), a verdict and a reason. A failing run prints the
//! entries it needs.

mod common;
#[macro_use]
#[path = "support/slices.rs"]
mod slices;

use std::collections::BTreeSet;
use std::io::Read;

use quiddity::Part;
use quiddity::features::section_recess_family::build_section_recess_document;
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const KNOWN: &str = "captured/section_recesses/known_differences.json";

fn dir() -> std::path::PathBuf {
    common::fixtures().join("captured/section_recesses")
}

fn captured() -> &'static Value {
    static CAPTURED: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURED.get_or_init(|| {
        let mut text = String::new();
        flate2::read::GzDecoder::new(std::fs::File::open(dir().join("documents.json.gz")).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        serde_json::from_str(&text).unwrap()
    })
}

fn runs() -> &'static [Value] {
    captured()["runs"].as_array().unwrap()
}

/// Every run's file, in capture order: what the test is sliced by.
fn run_files() -> Vec<String> {
    runs()
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect()
}

fn read(source: &str, file: &str) -> Option<Part> {
    let path = match source {
        "test" => dir().join(file),
        "fixture" => common::fixtures().join(file),
        _ => common::corpus_dir()?.join(file),
    };
    Some(read_step_file(&path).unwrap_or_else(|e| panic!("{file}: {e}")))
}

/// An occurrence's or refusal's region: its body and constituent faces.
fn region(item: &Value) -> Value {
    json!([item["body"], item["evidence"]["constituent_faces"]])
}

/// A refusal's key: its body and both evidence lists.
fn refusal_key(item: &Value) -> Value {
    json!([
        item["body"],
        item["evidence"]["defining_faces"],
        item["evidence"]["constituent_faces"]
    ])
}

/// A pattern with its member indices replaced by their occurrences' regions.
fn pattern_by_region(pattern: &Value, occurrences: &[Value]) -> Value {
    let mut p = pattern.clone();
    p["members"] = json!(
        pattern["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| region(&occurrences[m.as_u64().unwrap() as usize]))
            .collect::<Vec<_>>()
    );
    p
}

/// The differences between two documents, each as (identity, description).
fn differences(file: &str, got: &Value, want: &Value) -> Vec<(Value, String)> {
    let mut out = Vec::new();
    let mut push = |item: &str, key: Value, kind: &str, text: String| {
        out.push((
            json!({"file": file, "item": item, "key": key, "kind": kind}),
            format!("{file} {item} {kind}: {text}"),
        ));
    };
    if got.get("refused").is_some() || want.get("refused").is_some() {
        if got != want {
            push(
                "document",
                Value::Null,
                "differs",
                format!("port {got}, python {want}"),
            );
        }
        return out;
    }
    for field in ["schema_version", "reference_scope", "bodies", "faces"] {
        if got[field] != want[field] {
            push(
                "document",
                json!(field),
                "differs",
                format!("port {}, python {}", got[field], want[field]),
            );
        }
    }
    let occ = |d: &Value| d["occurrences"].as_array().unwrap().clone();
    let (got_occ, want_occ) = (occ(got), occ(want));
    let strip = |o: &Value| {
        let mut o = o.clone();
        o["index"] = Value::Null;
        o
    };
    for w in &want_occ {
        match got_occ.iter().find(|g| region(g) == region(w)) {
            None => push("occurrence", region(w), "missing", w.to_string()),
            Some(g) if !common::same(&strip(g), &strip(w)) => push(
                "occurrence",
                region(w),
                "differs",
                common::diff(&strip(g), &strip(w)),
            ),
            Some(_) => {}
        }
    }
    for g in &got_occ {
        if !want_occ.iter().any(|w| region(w) == region(g)) {
            push("occurrence", region(g), "extra", g.to_string());
        }
    }
    // The order among the occurrences both publish.
    let common_order = |a: &[Value], b: &[Value]| -> Vec<Value> {
        a.iter()
            .map(region)
            .filter(|r| b.iter().any(|o| &region(o) == r))
            .collect()
    };
    if common_order(&got_occ, &want_occ) != common_order(&want_occ, &got_occ) {
        push(
            "order",
            Value::Null,
            "differs",
            format!(
                "port {:?}, python {:?}",
                common_order(&got_occ, &want_occ),
                common_order(&want_occ, &got_occ)
            ),
        );
    }
    let refusals = |d: &Value| -> Vec<Value> { d["refusals"].as_array().unwrap().clone() };
    let (got_ref, want_ref) = (refusals(got), refusals(want));
    for w in &want_ref {
        if !got_ref.contains(w) {
            push("refusal", refusal_key(w), "missing", w.to_string());
        }
    }
    for g in &got_ref {
        if !want_ref.contains(g) {
            push("refusal", refusal_key(g), "extra", g.to_string());
        }
    }
    let patterns = |d: &Value, o: &[Value]| -> Vec<Value> {
        d["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| pattern_by_region(p, o))
            .collect()
    };
    let (got_pat, want_pat) = (patterns(got, &got_occ), patterns(want, &want_occ));
    for w in &want_pat {
        match got_pat.iter().find(|g| g["members"] == w["members"]) {
            None => push("pattern", w["members"].clone(), "missing", w.to_string()),
            Some(g) if !common::same(g, w) => push(
                "pattern",
                w["members"].clone(),
                "differs",
                common::diff(g, w),
            ),
            Some(_) => {}
        }
    }
    for g in &got_pat {
        if !want_pat.iter().any(|w| w["members"] == g["members"]) {
            push("pattern", g["members"].clone(), "extra", g.to_string());
        }
    }
    out
}

/// Test parts on which the port's recognition itself (the passages' entry-treatment proofs,
/// before any section-recess projection) takes 100 to 400 s, against Python's 7 to 18 s: compared
/// only when `QUIDDITY_SLOW_SECTION_RECESSES` is set (all agreed when last run).
const SLOW: [&str; 7] = [
    "parts/00eb6f057ad5d83c.step.gz",
    "parts/14ba78f684b0e385.step.gz",
    "parts/160a9f5d03a23b16.step.gz",
    "parts/3a4d47e864aef62a.step.gz",
    "parts/8086e3e719e1797d.step.gz",
    "parts/ce93e738613f41b1.step.gz",
    "parts/f3cf6fbffaeacd1a.step.gz",
];

/// Whether a run is compared: Python finished it, and it is not a slow part left out.
fn compared(run: &Value) -> bool {
    let file = run["file"].as_str().unwrap();
    run["document"].get("timeout").is_none()
        && (!SLOW.contains(&file) || std::env::var_os("QUIDDITY_SLOW_SECTION_RECESSES").is_some())
}

/// What one run compared.
#[derive(Default)]
struct Tally {
    documents: usize,
    occurrences: usize,
    refusals: usize,
    patterns: usize,
    refused: usize,
    timed_out: Vec<String>,
    slow: Vec<String>,
    found: Vec<(Value, String)>,
}

fn replay(run: &Value) -> Tally {
    let file = run["file"].as_str().unwrap();
    let want = &run["document"];
    let mut tally = Tally::default();
    if want.get("timeout").is_some() {
        tally.timed_out.push(file.to_string());
        return tally;
    }
    if !compared(run) {
        tally.slow.push(file.to_string());
        return tally;
    }
    let Some(part) = read(run["source"].as_str().unwrap(), file) else {
        return tally;
    };
    // `recognise` panics where Python's aggregate raises in a family (passages, oriented
    // slots, pads): that is the port's refusal of the whole document.
    let started = std::time::Instant::now();
    let got = match std::panic::catch_unwind(|| build_section_recess_document(&part)) {
        Ok(Ok(document)) => serde_json::to_value(&document).unwrap(),
        Ok(Err(e)) => json!({ "refused": format!("ValueError: {}", e.0) }),
        Err(panic) => json!({
            "refused": format!(
                "panicked: {}",
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default()
            )
        }),
    };
    let seconds = started.elapsed().as_secs();
    if seconds >= 30 {
        eprintln!("{file}: {seconds} s");
    }
    tally.documents = 1;
    if want.get("refused").is_some() {
        tally.refused = 1;
    } else {
        let count = |key: &str| want[key].as_array().unwrap().len();
        tally.occurrences = count("occurrences");
        tally.refusals = count("refusals");
        tally.patterns = count("patterns");
    }
    tally.found = differences(file, &got, want);
    tally
}

fn require_corpus() {
    if common::corpus_dir().is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
}

/// One slice of the runs (`tests/support/slices.rs`): every document compared, every
/// difference matched exactly against the listed entries of the slice's files.
fn documents_agree(k: usize, n: usize) {
    require_corpus();
    let files = run_files();
    let mine = slices::slice(&files, k, n);
    let selected: Vec<&Value> = runs()
        .iter()
        .filter(|r| mine.iter().any(|f| r["file"] == f.as_str()))
        .collect();
    let tallies = common::parallel::map(&selected, |run| replay(run));
    let ran: BTreeSet<String> = selected
        .iter()
        .filter(|r| common::corpus_dir().is_some() || r["source"] != "corpus")
        .filter(|r| compared(r))
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect();
    let mut total = Tally::default();
    for t in tallies {
        total.documents += t.documents;
        total.occurrences += t.occurrences;
        total.refusals += t.refusals;
        total.patterns += t.patterns;
        total.refused += t.refused;
        total.timed_out.extend(t.timed_out);
        total.slow.extend(t.slow);
        total.found.extend(t.found);
    }
    eprintln!(
        "slice {k}/{n}: {} documents ({} refused by Python), {} occurrences, {} refusals, {} \
         patterns; {} differences; not captured (Python timed out): {:?}; slow parts not \
         compared: {:?}",
        total.documents,
        total.refused,
        total.occurrences,
        total.refusals,
        total.patterns,
        total.found.len(),
        total.timed_out,
        total.slow
    );

    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts("section_recesses/known_differences.json", known);
    let listed: Vec<&Value> = known
        .iter()
        .filter(|e| e["item"] != "invariance")
        .filter(|e| slices::owns(&files, k, n, e["file"].as_str().unwrap()))
        .filter(|e| ran.contains(e["file"].as_str().unwrap()))
        .collect();
    let mut problems = Vec::new();
    let mut seen = vec![false; listed.len()];
    for (identity, text) in &total.found {
        let position = listed.iter().position(|e| {
            identity
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| &e[key] == value)
        });
        match position {
            Some(i) if seen[i] => problems.push(format!("listed twice: {identity}")),
            Some(i) => seen[i] = true,
            None => problems.push(format!("unlisted difference: {text}\n  entry: {identity}")),
        }
    }
    for (i, e) in listed.iter().enumerate() {
        if !seen[i] {
            problems.push(format!("listed entry no longer differs: {e}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    documents_agree_with_python,
    super::documents_agree,
    files: super::run_files,
    [0 => slice_0, 1 => slice_1, 2 => slice_2, 3 => slice_3]
);

/// The capture covers what it says: the corpus, both golden fixtures, and test parts, each run
/// answered (a document, a refusal or a timeout).
#[test]
fn capture_covers_the_corpus() {
    let corpus: BTreeSet<String> = common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_string())
        .collect();
    let captured: BTreeSet<String> = runs()
        .iter()
        .filter(|r| r["source"] == "corpus")
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(captured, corpus);
    assert_eq!(
        runs().iter().filter(|r| r["source"] == "fixture").count(),
        2
    );
    assert!(runs().iter().any(|r| r["source"] == "test"));
    for run in runs() {
        let d = &run["document"];
        assert!(
            d.get("occurrences").is_some()
                || d.get("refused").is_some()
                || d.get("timeout").is_some(),
            "{}: unanswered",
            run["file"]
        );
    }
}

/// `tests/invariance.rs`'s translation and quarter turn about z.
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);
const MOTIONS: [Motion; 2] = [
    (
        "translate",
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        [123.456, -78.9, 41.3],
    ),
    (
        "rot_z90",
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0; 3],
    ),
];

/// The corpus parts on which Python publishes an occurrence or a refusal.
fn moved_files() -> Vec<String> {
    runs()
        .iter()
        .filter(|r| r["source"] == "corpus")
        .filter(|r| {
            ["occurrences", "refusals"]
                .iter()
                .any(|k| r["document"][k].as_array().is_some_and(|a| !a.is_empty()))
        })
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect()
}

/// What a document says about faces: each occurrence's body, evidence and classification, and
/// each refusal, sorted; or the refusal of the whole document.
fn face_reading(part: &Part) -> Value {
    match build_section_recess_document(part) {
        Ok(document) => {
            let d = serde_json::to_value(&document).unwrap();
            let mut occurrences: Vec<String> = d["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| json!([o["body"], o["evidence"], o["classification"]]).to_string())
                .collect();
            let mut refusals: Vec<String> = d["refusals"]
                .as_array()
                .unwrap()
                .iter()
                .map(Value::to_string)
                .collect();
            occurrences.sort();
            refusals.sort();
            json!({"occurrences": occurrences, "refusals": refusals})
        }
        Err(e) => json!({ "refused": e.0 }),
    }
}

/// The documents under two rigid motions: on every corpus part where Python publishes something,
/// the moved part must give the same occurrences and refusals on the same faces with the same
/// classification (their geometry is re-expressed by the motion and is not compared here). Each
/// difference is listed with `item` `invariance` and its motion as `key`.
fn documents_invariant(k: usize, n: usize) {
    require_corpus();
    let Some(corpus) = common::corpus_dir() else {
        return;
    };
    let files = moved_files();
    let mine = slices::slice(&files, k, n);
    let readings: Vec<(usize, Option<&Motion>)> = (0..mine.len())
        .flat_map(|i| {
            std::iter::once(None)
                .chain(MOTIONS.iter().map(Some))
                .map(move |m| (i, m))
        })
        .collect();
    let mut read_all = common::parallel::map(&readings, |&(i, motion)| {
        let path = corpus.join(&mine[i]);
        let part = match motion {
            None => read_step_file(&path),
            Some((_, r, t)) => {
                let placement: Placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]]);
                read_step_file_placed(&path, &placement)
            }
        }
        .unwrap_or_else(|e| panic!("{}: {e}", mine[i]));
        face_reading(&part)
    })
    .into_iter();
    let mut found = Vec::new();
    for file in &mine {
        let unmoved = read_all.next().unwrap();
        for (name, _, _) in &MOTIONS {
            let moved = read_all.next().unwrap();
            if moved != unmoved {
                found.push((
                    json!({"file": file, "item": "invariance", "key": name, "kind": "differs"}),
                    format!("{file} {name}: moved {moved}, unmoved {unmoved}"),
                ));
            }
        }
    }
    eprintln!(
        "slice {k}/{n}: {} parts under {} motions; {} differ",
        mine.len(),
        MOTIONS.len(),
        found.len()
    );
    let known = common::load(KNOWN);
    let listed: Vec<&Value> = known
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["item"] == "invariance")
        .filter(|e| mine.iter().any(|f| e["file"] == f.as_str()))
        .collect();
    let mut problems = Vec::new();
    let mut seen = vec![false; listed.len()];
    for (identity, text) in &found {
        match listed.iter().position(|e| {
            identity
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| &e[key] == value)
        }) {
            Some(i) if seen[i] => problems.push(format!("listed twice: {identity}")),
            Some(i) => seen[i] = true,
            None => problems.push(format!("unlisted difference: {text}\n  entry: {identity}")),
        }
    }
    for (i, e) in listed.iter().enumerate() {
        if !seen[i] {
            problems.push(format!("listed entry no longer differs: {e}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    documents_do_not_depend_on_placement,
    super::documents_invariant,
    files: super::moved_files,
    [0 => slice_0, 1 => slice_1]
);
