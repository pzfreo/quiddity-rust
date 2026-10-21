//! Rust section-recess document snapshots and unchanged motion checks over the complete legacy input selection.

use super::common;
use super::slices;

use std::collections::BTreeSet;
use std::io::Read;

use quiddity::Part;
use quiddity::features::section_recess_family::build_section_recess_document;
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const KNOWN: &str = "references/recess_document_motion.json";

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

/// Whether a run is compared: Python finished it.
fn compared(run: &Value) -> bool {
    run["snapshot_input"] == true
}

fn replay(run: &Value) -> Option<Value> {
    if !compared(run) {
        return None;
    }
    let file = run["file"].as_str().unwrap();
    let part = read(run["source"].as_str().unwrap(), file)?;
    let result = std::panic::catch_unwind(|| build_section_recess_document(&part));
    Some(match result {
        Ok(Ok(document)) => serde_json::to_value(document).unwrap(),
        Ok(Err(e)) => json!({"refused":e.0}),
        Err(panic) => {
            json!({"panicked":panic.downcast_ref::<String>().cloned().or_else(||panic.downcast_ref::<&str>().map(|s|s.to_string())).unwrap_or_default()})
        }
    })
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
    common::parallel::map(&selected, |run| {
        if let Some(actual) = replay(run) {
            common::snapshot::check(
                &common::fixtures(),
                &format!(
                    "recess_documents/{}/{}",
                    run["source"].as_str().unwrap(),
                    run["file"].as_str().unwrap()
                ),
                &actual,
            );
        }
    });
}

sliced!(
    rust_snapshot_documents,
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
    assert!(
        runs()
            .iter()
            .all(|r| r["snapshot_input"].is_boolean() && r["motion_input"].is_boolean())
    );
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
        .filter(|r| r["motion_input"] == true)
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
