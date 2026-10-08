//! The versioned recognition document (`quiddity::recognition`, review finding M5): every
//! `Features` family has Python's record type, records are in `Features`' family order with
//! ascending faces within the part, and the document round-trips through JSON and is the same
//! from run to run.

mod common;

use std::path::{Path, PathBuf};

use quiddity::correspondence;
use quiddity::features;
use quiddity::recognition::{
    self, RECORD_TYPES, RecognitionDocument, SCHEMA_VERSION, families, record_types,
};

fn fixture(name: &str) -> PathBuf {
    common::fixtures().join(name)
}

/// The document of the STEP file at *path*.
fn document(path: &Path) -> RecognitionDocument {
    let part = quiddity::read_step_file(path).unwrap();
    recognition::document(&correspondence::recognise(&part), part.faces.len())
}

#[test]
fn every_family_has_a_record_type() {
    let part = quiddity::read_step_file(&fixture("filleted_plate_with_holes.step")).unwrap();
    let keys: Vec<String> = families(&features::recognise(&part))
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    let missing: Vec<String> = keys
        .iter()
        .filter(|key| record_types(key).is_none())
        .map(|key| {
            format!(
                "family {key} has no record type: add (\"{key}\", &[\"<Python record class>\"]) \
                 to RECORD_TYPES in src/recognition.rs, at its place in Features' order (one \
                 class per variant for an untagged family, picked in record_type)"
            )
        })
        .collect();
    assert!(missing.is_empty(), "{}", missing.join("\n"));
    let table: Vec<&str> = RECORD_TYPES.iter().map(|(f, _)| *f).collect();
    assert_eq!(
        table, keys,
        "RECORD_TYPES must list exactly Features' families, in its order"
    );
}

#[test]
fn records_are_ordered_typed_and_on_the_parts_faces() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(common::fixtures())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "step"))
        .collect();
    paths.sort();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let problems: Vec<String> = std::thread::scope(|s| {
        let chunks: Vec<_> = paths
            .chunks(paths.len().div_ceil(threads))
            .map(|chunk| {
                s.spawn(move || {
                    let mut out = Vec::new();
                    for path in chunk {
                        let name = path.file_name().unwrap().to_string_lossy();
                        let Ok(part) = quiddity::read_step_file(path) else {
                            continue;
                        };
                        let doc = recognition::document(
                            &correspondence::recognise(&part),
                            part.faces.len(),
                        );
                        check(&name, &doc, part.faces.len(), &mut out);
                    }
                    out
                })
            })
            .collect();
        chunks.into_iter().flat_map(|c| c.join().unwrap()).collect()
    });
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// What is wrong with *doc*, the document of a part of *face_count* faces.
fn check(name: &str, doc: &RecognitionDocument, face_count: usize, out: &mut Vec<String>) {
    if doc.schema_version != SCHEMA_VERSION || doc.face_count != face_count {
        out.push(format!("{name}: version or face count wrong"));
    }
    let rank = |family: &str| RECORD_TYPES.iter().position(|(f, _)| *f == family);
    let mut last = (0, 0);
    for r in &doc.records {
        let Some(family) = rank(&r.family) else {
            out.push(format!("{name}: {} is in no family", r.id));
            continue;
        };
        let n: usize = r.id.rsplit('/').next().unwrap().parse().unwrap();
        if r.id != format!("{}/{n}", r.family) || (family, n) < last {
            out.push(format!("{name}: {} is out of order", r.id));
        }
        last = (family, n + 1);
        if !record_types(&r.family)
            .unwrap()
            .contains(&r.record_type.as_str())
        {
            out.push(format!("{name}: {} is a {}", r.id, r.record_type));
        }
        if r.faces.is_empty()
            || !r.faces.windows(2).all(|w| w[0] < w[1])
            || r.faces.last().is_some_and(|&f| f >= face_count)
        {
            out.push(format!("{name}: {} has faces {:?}", r.id, r.faces));
        }
    }
}

#[test]
fn the_document_round_trips_and_is_deterministic() {
    for (name, types) in [
        (
            "filleted_plate_with_holes.step",
            &["Fillet", "HoleRecord"][..],
        ),
        (
            "golden_bolt_circle_and_rectangular_grid.step",
            &["BoltCircle", "RectGrid"][..],
        ),
        ("golden_gusset_rib_patterns.step", &["GussetRib"][..]),
    ] {
        let path = fixture(name);
        let doc = document(&path);
        for t in types {
            assert!(
                doc.records.iter().any(|r| r.record_type == *t),
                "{name}: no {t}"
            );
        }
        let text = serde_json::to_string_pretty(&doc).unwrap();
        let back: RecognitionDocument = serde_json::from_str(&text).unwrap();
        assert_eq!(back, doc, "{name}: the document does not round-trip");
        let again = serde_json::to_string_pretty(&document(&path)).unwrap();
        assert_eq!(again, text, "{name}: two runs differ");
    }
}
