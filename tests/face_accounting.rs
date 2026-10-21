//! Complete membership and a reproducible, uncommitted census of the goldens and corpus.
use std::collections::BTreeSet;
use std::path::PathBuf;

use quiddity::recognition::{FaceAccounting, RecognitionDocument};

fn check(doc: &RecognitionDocument) -> [usize; 5] {
    let groups = doc.face_accounting.as_ref().unwrap();
    assert_eq!(groups.len(), doc.face_count);
    let mut counts = [0; 5]; // feature, body, refused, unexplained, unclaimed before
    for (face, group) in groups.iter().enumerate() {
        let records: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.faces.contains(&face))
            .collect();
        let feature_ids: Vec<_> = records
            .iter()
            .filter(|r| !r.is_refusal())
            .map(|r| r.id.clone())
            .collect();
        let refused: Vec<_> = records.iter().filter(|r| r.is_refusal()).collect();
        if feature_ids.is_empty() {
            counts[4] += 1;
        }
        match group {
            FaceAccounting::Feature { record_ids } => {
                assert!(!record_ids.is_empty());
                assert_eq!(*record_ids, feature_ids);
                counts[0] += 1;
            }
            FaceAccounting::Refused { refusals } => {
                assert!(feature_ids.is_empty());
                assert_eq!(refusals.len(), refused.len());
                assert!(!refusals.is_empty());
                for (reference, record) in refusals.iter().zip(refused) {
                    assert_eq!(reference.record_id, record.id);
                    assert_eq!(
                        reference.reason,
                        record.parameters["reason"].as_str().unwrap()
                    );
                }
                counts[2] += 1;
            }
            FaceAccounting::Body { .. } => {
                assert!(records.is_empty());
                counts[1] += 1;
            }
            FaceAccounting::Unexplained => {
                assert!(records.is_empty());
                counts[3] += 1;
            }
        }
    }
    assert_eq!(counts[..4].iter().sum::<usize>(), doc.face_count);
    counts
}

#[test]
fn complete_partition_on_all_goldens_and_recorded_corpus() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<_> = std::fs::read_dir(&fixtures)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("golden_")
                && p.extension().is_some_and(|e| e == "step")
        })
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 34);
    if let Some(root) = std::env::var_os("QUIDDITY_CORPUS").filter(|r| r != "off") {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/corpus.json")).unwrap();
        paths.extend(
            corpus["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| PathBuf::from(&root).join(e["file"].as_str().unwrap())),
        );
    } else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS required"
        );
    }
    let mut total = [0; 5];
    for path in paths {
        let (part, recognition, _) = quiddity::serve::recognise_step(
            path.to_str().unwrap(),
            &quiddity::kernel::step::IDENTITY,
        )
        .unwrap();
        let doc = quiddity::recognition::document(&recognition, part.faces.len());
        let counts = check(&doc);
        for (sum, count) in total.iter_mut().zip(counts) {
            *sum += count;
        }
        eprintln!("ACCOUNTING {} {counts:?}", path.display());
        let body: BTreeSet<_> = doc
            .face_accounting
            .as_ref()
            .unwrap()
            .iter()
            .enumerate()
            .filter_map(|(f, g)| matches!(g, FaceAccounting::Body { .. }).then_some(f))
            .collect();
        eprintln!("BODY {} {body:?}", path.display());
        if let quiddity::frames::RecordFrame::Inferred(frame) = &doc.frame {
            let working = part.transformed(&frame.normalisation()).unwrap();
            for f in body {
                let face = &working.faces[f];
                let bounds = working.solid_bounds(face.solid.unwrap());
                let vertices: Vec<_> = face
                    .loops
                    .iter()
                    .filter(|lp| lp.edges.len() == 4)
                    .map(|lp| {
                        lp.edges
                            .iter()
                            .map(|&(e, forward)| {
                                let edge = &working.edges[e];
                                if forward { edge.start } else { edge.end }
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                eprintln!(
                    "GEOMETRY {} face={f} normal={:?} solid_box={bounds:?} loops={} rectangle_vertices={vertices:?}",
                    path.display(),
                    working.face_normal(f, 0.0, 0.0),
                    face.loops.len()
                );
            }
        }
        let back: RecognitionDocument =
            serde_json::from_value(serde_json::to_value(&doc).unwrap()).unwrap();
        assert_eq!(back, doc);
    }
    eprintln!("ACCOUNTING TOTAL {total:?}");
}

#[test]
fn legacy_missing_accounting_is_unknown() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden_simple_through_hole.step");
    let (output, _) = quiddity::serve::recognise_file(path.to_str().unwrap()).unwrap();
    for version in [3, 4] {
        let mut old = serde_json::to_value(&output.document).unwrap();
        old["schema_version"] = format!("quiddity-rust/recognition/{version}").into();
        old.as_object_mut().unwrap().remove("face_accounting");
        let old: RecognitionDocument = serde_json::from_value(old).unwrap();
        assert!(old.face_accounting.is_none());
        assert!(
            serde_json::to_value(old)
                .unwrap()
                .get("face_accounting")
                .is_none()
        );
    }
}
