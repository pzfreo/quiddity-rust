//! Section recesses in the document handed to draftwright: Python's captured records on its
//! 30 quality parts, caller and inferred frames, and in-run additivity of publication.

mod common;

use std::io::Read;
use std::sync::OnceLock;

use quiddity::correspondence;
use quiddity::kernel::step::IDENTITY;
use quiddity::recognition::{self, RecognitionDocument};
use quiddity::serve;
use serde_json::{Value, json};

const KNOWN: &str = "captured/section_recesses/known_differences.json";

struct Run {
    baseline: Value,
    caller_unpublished: RecognitionDocument,
    framed_unpublished: RecognitionDocument,
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
                caller_unpublished: without_section_recess_publication(&caller, part.faces.len()),
                framed_unpublished: without_section_recess_publication(&framed, part.faces.len()),
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
                // Keep proved Rust-first corrections: omit NIST slots mispublished by
                // Python's cap boolean, or apply the explicit per-field geometry corrections.
                assert_eq!(verdict["verdict"], "rust-correct");
                used.push(i);
                match verdict["kind"].as_str().unwrap() {
                    "missing" => {}
                    "differs" => {
                        // Rust-first corrections are explicit per-field expectations. Check
                        // their frozen Python inputs and compare the complete corrected record.
                        let mut corrected = record.clone();
                        for change in verdict["corrections"].as_array().unwrap() {
                            let path = change["path"].as_str().unwrap();
                            assert_eq!(corrected.pointer(path), Some(&change["python"]));
                            *corrected.pointer_mut(path).unwrap() = change["rust"].clone();
                        }
                        corrected["index"] = json!(expected.len());
                        expected.push(corrected);
                    }
                    other => panic!("unsupported quality-part correction: {other}"),
                }
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

/// Build through the normal document builder with only section-recess publication disabled.
/// Recognition, numeric measurements, fingerprints and frame mapping are the same run's.
fn without_section_recess_publication(
    recognition: &correspondence::Recognition,
    face_count: usize,
) -> RecognitionDocument {
    let mut unpublished = recognition.clone();
    unpublished.features.section_recesses.clear();
    unpublished.features.section_recess_refusals.clear();
    unpublished.features.section_recess_patterns.clear();
    recognition::document(&unpublished, face_count)
}

/// Project both documents onto the pre-publication contract. Later additive trust fields
/// and void diagnostics have their own payload checks; no measured field is normalised.
/// Keep record order, IDs, face claims, parameters, local paths, frame and package version.
fn existing_document_bytes(document: &RecognitionDocument) -> Vec<u8> {
    let mut value = serde_json::to_value(document).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("source_sha256");
    object.remove("run_mode");
    let records = object["records"].as_array_mut().unwrap();
    records.retain(|r| {
        !matches!(
            r["family"].as_str().unwrap(),
            "section_recesses" | "section_recess_patterns"
        ) && r["record_type"] != "InteriorVoidRefusal"
    });
    for record in records {
        record.as_object_mut().unwrap().remove("evidence_status");
    }
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn section_recess_publication_is_additive() {
    for run in runs() {
        let file = run.baseline["file"].as_str().unwrap();
        for (name, published, unpublished) in [
            ("caller", &run.caller, &run.caller_unpublished),
            ("framed", &run.framed, &run.framed_unpublished),
        ] {
            assert!(
                unpublished.records.iter().all(|r| !matches!(
                    r.family.as_str(),
                    "section_recesses" | "section_recess_patterns"
                )),
                "{file}: {name} unpublished path emitted section recesses"
            );
            assert_eq!(
                existing_document_bytes(published),
                existing_document_bytes(unpublished),
                "{file}: {name} section-recess publication changed existing document bytes"
            );
        }
    }
}

/// The payload actually published, including patterns that used to be discarded. The Python
/// capture includes rectangular, obround and rounded profiles, arrays and grids.
#[test]
fn pattern_publication_matches_python() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        std::fs::File::open(common::fixtures().join("captured/section_recesses/documents.json.gz"))
            .unwrap(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let capture: Value = serde_json::from_str(&text).unwrap();
    let mut checked = 0;
    for run in capture["runs"].as_array().unwrap().iter().filter(|r| {
        r["source"] == "test"
            && r["document"]["patterns"]
                .as_array()
                .is_some_and(|p| !p.is_empty())
    }) {
        let path = common::fixtures()
            .join("captured/section_recesses")
            .join(run["file"].as_str().unwrap());
        let part = quiddity::read_step_file(&path).unwrap();
        let rec = correspondence::recognise(&part);
        let doc = recognition::document(&rec, part.faces.len());
        assert_eq!(doc.schema_version, "quiddity-rust/recognition/3");
        let patterns: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "section_recess_patterns")
            .map(|r| Value::Object(r.parameters.clone()))
            .collect();
        assert!(
            common::same(&json!(patterns), &run["document"]["patterns"]),
            "{}",
            run["file"]
        );
        for p in doc
            .records
            .iter()
            .filter(|r| r.family == "section_recess_patterns")
        {
            let mut faces: Vec<usize> = Vec::new();
            for m in p.parameters["members"].as_array().unwrap() {
                let index = m.as_u64().unwrap();
                let member = doc
                    .records
                    .iter()
                    .find(|r| {
                        r.family == "section_recesses"
                            && r.record_type == "SectionRecess"
                            && r.parameters["index"] == index
                    })
                    .unwrap();
                faces.extend(&member.faces);
            }
            faces.sort_unstable();
            faces.dedup();
            assert_eq!(p.faces, faces);
        }
        checked += 1;
    }
    assert!(checked >= 2);
}

#[test]
fn k3_degraded_records_remain_pythons_in_the_published_document() {
    let Some(corpus) = common::corpus_dir() else {
        return;
    };
    for (name, count) in [("13975", 0), ("14052", 1)] {
        let part =
            quiddity::read_step_file(&corpus.join(format!("inventory_refusal/{name}.step.gz")))
                .unwrap();
        let rec = correspondence::recognise(&part);
        assert!(rec.features.section_recess_refusal.is_none());
        let doc = recognition::document(&rec, part.faces.len());
        let records: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "section_recesses" && r.record_type == "SectionRecess")
            .collect();
        assert_eq!(records.len(), count, "{name}");
        if count == 1 {
            assert_eq!(
                records[0].parameters["geometry"]["profile"]["boundary"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
        }
    }
}
