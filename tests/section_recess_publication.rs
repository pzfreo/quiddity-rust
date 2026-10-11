//! Section recesses in the document handed to draftwright: Python's captured records on its
//! 30 quality parts, caller and inferred frames, and byte preservation of the previous document.

mod common;

use std::io::Read;
use std::sync::OnceLock;

use quiddity::correspondence;
use quiddity::kernel::step::IDENTITY;
use quiddity::recognition::{self, RecognitionDocument};
use quiddity::serve;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const KNOWN: &str = "captured/section_recesses/known_differences.json";

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Run {
    baseline: Value,
    features: String,
    caller: RecognitionDocument,
    framed: RecognitionDocument,
    framed_recesses: Value,
}

fn runs() -> &'static [Run] {
    static RUNS: OnceLock<Vec<Run>> = OnceLock::new();
    RUNS.get_or_init(|| {
        let Some(corpus) = common::corpus_dir() else {
            assert!(
                std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
                "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
            );
            return Vec::new();
        };
        let baseline = common::load("section_recess_publication_baseline.json");
        let rows = baseline["runs"].as_array().unwrap();
        assert_eq!(rows.len(), 30);
        common::parallel::map(rows, |baseline| {
            let file = baseline["file"].as_str().unwrap();
            let path = corpus.join(file);
            let part = quiddity::read_step_file(&path).unwrap();
            let caller = correspondence::recognise(&part);
            assert!(caller.features.section_recess_refusal.is_none(), "{file}");
            let caller_doc = recognition::document(&caller, part.faces.len());
            let (_, framed, _) = serve::recognise_step(path.to_str().unwrap(), &IDENTITY).unwrap();
            Run {
                baseline: baseline.clone(),
                features: serde_json::to_string(&caller.features).unwrap(),
                caller: caller_doc,
                framed: recognition::document(&framed, part.faces.len()),
                framed_recesses: json!(
                    recognition::families(&framed.features)
                        .into_iter()
                        .find(|(f, _)| f == "section_recesses")
                        .unwrap()
                        .1
                ),
            }
        })
    })
}

fn recesses(doc: &RecognitionDocument) -> Vec<Value> {
    doc.records
        .iter()
        .filter(|r| r.family == "section_recesses")
        .enumerate()
        .map(|(index, r)| {
            assert_eq!(r.id, format!("section_recesses/{index}"));
            assert_eq!(
                r.record_type,
                if r.parameters.contains_key("reason") {
                    "SectionRecessRefusal"
                } else {
                    "SectionRecess"
                }
            );
            assert_eq!(json!(r.faces), r.parameters["evidence"]["defining_faces"]);
            Value::Object(r.parameters.clone())
        })
        .collect()
}

#[test]
fn quality_part_section_recesses_match_python() {
    if runs().is_empty() {
        return;
    }
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        std::fs::File::open(common::fixtures().join("captured/section_recesses/documents.json.gz"))
            .unwrap(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let python: Value = serde_json::from_str(&text).unwrap();
    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts(KNOWN, known);
    let mut used = Vec::new();
    let mut counts = [0; 4];
    let mut parts = [0; 4];
    let mut refusals = 0;
    let mut refusal_parts = 0;
    for run in runs() {
        let file = run.baseline["file"].as_str().unwrap();
        let capture = python["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["source"] == "corpus" && r["file"] == file)
            .unwrap_or_else(|| panic!("no Python capture for {file}"));
        let mut expected = Vec::new();
        for record in capture["document"]["occurrences"].as_array().unwrap() {
            let key = json!([record["body"], record["evidence"]["constituent_faces"]]);
            if let Some((i, verdict)) = known
                .iter()
                .enumerate()
                .find(|(_, k)| k["file"] == file && k["item"] == "occurrence" && k["key"] == key)
            {
                // Keep the proved kernel corrections: these NIST slots are capped at one end,
                // though Python's coplanar boolean incorrectly publishes two open ends.
                assert_eq!(verdict["kind"], "missing");
                assert_eq!(verdict["verdict"], "rust-correct");
                used.push(i);
            } else {
                let mut record = record.clone();
                record["index"] = json!(expected.len());
                expected.push(record);
            }
        }
        let expected_refusals = capture["document"]["refusals"].as_array().unwrap();
        refusals += expected_refusals.len();
        refusal_parts += usize::from(!expected_refusals.is_empty());
        expected.extend(expected_refusals.iter().cloned());
        let got = recesses(&run.caller);
        assert!(
            common::same(&json!(got), &json!(expected)),
            "{file}: {}",
            common::diff(&json!(got), &json!(expected))
        );
        let mut present = [false; 4];
        for r in got.iter().filter(|r| r.get("reason").is_none()) {
            let kind = r["classification"]["feature_kind"].as_str().unwrap();
            let i = ["pocket", "channel", "passage", "edge_open_recess"]
                .iter()
                .position(|k| *k == kind)
                .unwrap();
            counts[i] += 1;
            present[i] = true;
        }
        for (i, found) in present.into_iter().enumerate() {
            parts[i] += usize::from(found);
        }
    }
    assert_eq!((refusals, refusal_parts), (12, 5));
    let relevant: Vec<usize> = known
        .iter()
        .enumerate()
        .filter(|(_, k)| runs().iter().any(|r| r.baseline["file"] == k["file"]))
        .map(|(i, _)| i)
        .collect();
    used.sort_unstable();
    assert_eq!(used, relevant, "stale or unchecked quality-part verdict");
    eprintln!(
        "quality section recesses: counts {counts:?}, parts {parts:?}, verdicts {}",
        used.len()
    );
}

#[test]
fn inferred_frame_section_recesses_are_published() {
    if runs().is_empty() {
        return;
    }
    let mut published = 0;
    for run in runs() {
        let file = run.baseline["file"].as_str().unwrap();
        let got = json!(recesses(&run.framed));
        assert_eq!(
            got, run.framed_recesses,
            "{file}: framed records lost or changed"
        );
        published += got.as_array().unwrap().len();
        let bytes = serde_json::to_vec(&run.framed).unwrap();
        let back: RecognitionDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, run.framed, "{file}: document round trip");
    }
    assert!(published > 0, "no framed section recesses published");
}

#[test]
fn publication_preserves_existing_document_bytes() {
    for run in runs() {
        let file = run.baseline["file"].as_str().unwrap();
        // The new family is appended to Features: removing precisely that key must recover
        // its old bytes, including the order of every existing family and record.
        let start = run
            .features
            .find(",\"section_recesses\":")
            .unwrap_or_else(|| {
                panic!("{file}: Features does not publish the section_recesses key")
            });
        let old = run.features[..start].to_string() + "}";
        assert_eq!(
            hash(old.as_bytes()),
            run.baseline["features_sha256"],
            "{file}: old Features bytes"
        );
        for (name, doc) in [("caller", &run.caller), ("framed", &run.framed)] {
            let mut old = doc.clone();
            old.records.retain(|r| r.family != "section_recesses");
            assert_eq!(
                hash(&serde_json::to_vec(&old).unwrap()),
                run.baseline[format!("{name}_document_sha256")],
                "{file}: old {name} document bytes"
            );
        }
    }
}
