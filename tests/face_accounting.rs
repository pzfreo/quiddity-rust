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

#[test]
fn ctc01_e1_accounting_records_and_main_correspondence() {
    use quiddity::correspondence::{self, Fingerprints};
    use std::io::Read;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz");
    let (output, _) = quiddity::serve::recognise_file(path.to_str().unwrap()).unwrap();
    let doc = &output.document;
    assert_eq!(doc.schema_version, "quiddity-rust/recognition/6");
    use quiddity::recognition::BodyReason;
    assert_eq!(check(doc), [90, 20, 0, 7, 27]);
    let feature_faces = [
        0, 1, 2, 3, 5, 7, 8, 9, 12, 16, 19, 20, 21, 23, 25, 27, 28, 29, 31, 32, 34, 35, 37, 38, 40,
        41, 43, 44, 45, 50, 51, 52, 53, 56, 57, 58, 59, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
        74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96,
        97, 98, 99, 100, 101, 102, 103, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116,
    ];
    let mut expected = vec![FaceAccounting::Unexplained; 117];
    for (reason, faces) in [
        (BodyReason::StockBoxNotched, &[4, 24][..]),
        (
            BodyReason::StockBoxSplit,
            &[6, 22, 26, 46, 47, 49, 54, 55][..],
        ),
        (BodyReason::PlateEdge, &[10, 14, 18, 30, 36, 42][..]),
        (BodyReason::StockWall, &[48, 60, 61, 62][..]),
    ] {
        for &face in faces {
            expected[face] = FaceAccounting::Body { reason };
        }
    }
    for (face, group) in doc.face_accounting.as_ref().unwrap().iter().enumerate() {
        if feature_faces.contains(&face) {
            assert!(
                matches!(group, FaceAccounting::Feature { .. }),
                "face {face}"
            );
        } else {
            assert_eq!(*group, expected[face], "face {face}");
        }
    }
    let unexplained: Vec<_> = doc
        .face_accounting
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(f, g)| matches!(g, FaceAccounting::Unexplained).then_some(f))
        .collect();
    assert_eq!(unexplained, [11, 13, 15, 17, 33, 39, 104]);
    let part = quiddity::read_step_file(&path).unwrap();
    let near = |a: f64, b: f64| assert!((a - b).abs() < 1e-6, "{a} != {b}");
    // Verify issue face hypotheses on file-space geometry, not numbering alone.
    for f in [10, 14, 18, 30, 36, 42] {
        let b = part.face_bounds(f);
        near(b.min[2], -50.);
        near(b.max[2], 0.);
        near(b.min[1].abs(), 125.);
        near(b.min[1], b.max[1]);
    }
    for f in [8, 20, 28, 44] {
        let b = part.face_bounds(f);
        near(b.min[0].abs(), 250.);
        near(b.min[0], b.max[0]);
        near(b.min[2], -100.);
        near(b.max[2], 0.);
    }
    for f in [48, 60, 61, 62] {
        let b = part.face_bounds(f);
        near(b.min[1].abs(), 125.);
        near(b.min[1], b.max[1]);
        near(b.min[2], -100.);
        near(b.max[2], -60.);
    }
    for f in [31, 33, 35, 37, 39, 41] {
        let b = part.face_bounds(f);
        near(b.min[2], -50.);
        near(b.max[2], 0.);
        assert!(matches!(
            part.faces[f].surface,
            quiddity::kernel::geom::Surface::Plane { .. }
        ));
    }
    // Four additional V-notch flanks: oblique planes, full slab, turning smoothly
    // into R10 concave rounds 12/16. They are cut walls, not exposed plate edges.
    for (f, blend) in [(11, 12), (13, 12), (15, 16), (17, 16)] {
        let b = part.face_bounds(f);
        near(b.min[2], -50.);
        near(b.max[2], 0.);
        let n = part.face_normal(f, 0., 0.).unwrap();
        near(n[0].abs(), 3_f64.sqrt() / 2.);
        near(n[1], -0.5);
        assert_eq!(
            part.arc(f, blend),
            Some(quiddity::kernel::brep::Arc::Smooth)
        );
        assert_eq!(part.frame_points_outward(blend), Some(false));
    }
    for (f, x) in [(56, 30.), (57, 30.), (58, -30.), (59, -30.)] {
        let apex = part.faces[f].surface.cone_apex().unwrap();
        near(apex[0], x);
        near(apex[1], -73.9913938097245);
        near(apex[2], -25.);
    }
    for (f, x) in [(82, 235.), (86, 335.)] {
        let b = part.face_bounds(f);
        near(b.min[0], x);
        near(b.max[0], x);
        near(b.max[1] - b.min[1], 30.);
        near(b.min[2], -50.);
        near(b.max[2], 0.);
    }
    near(part.face_bounds(104).min[2], 50.);
    assert_eq!(
        doc.records
            .iter()
            .find(|r| r.id == "polygonal_bosses/0")
            .unwrap()
            .faces,
        [105, 106, 107, 108, 109, 110]
    );
    // Preserve existing step-level feature evidence on four rectangular-notch
    // side walls. Neither body rule may claim any of the six notch walls.
    for (f, id) in [
        (31, "step_levels/3"),
        (35, "step_levels/4"),
        (37, "step_levels/7"),
        (41, "step_levels/8"),
    ] {
        assert_eq!(
            doc.face_accounting.as_ref().unwrap()[f],
            FaceAccounting::Feature {
                record_ids: vec![id.into()]
            }
        );
    }
    for (id, faces) in [
        ("holes/1", vec![50, 51, 56, 57]),
        ("holes/2", vec![52, 53, 58, 59]),
        ("slots/1", vec![82, 84, 86, 88]),
    ] {
        assert_eq!(
            doc.records.iter().find(|r| r.id == id).unwrap().faces,
            faces
        );
    }
    // Independent origin/main ff1975e CLI output, on these exact compressed input
    // bytes. Keep the complete old face graph so this exercises real alignment
    // and assignment across the changed claims, rather than self-correspondence.
    let mut old = String::new();
    flate2::read::GzDecoder::new(
        &include_bytes!("fixtures/references/ctc01-main-fingerprints.json.gz")[..],
    )
    .read_to_string(&mut old)
    .unwrap();
    let old: Fingerprints = serde_json::from_str(&old).unwrap();
    let new = &output.recognition.fingerprints;
    assert_eq!(new.fingerprint_version, "quiddity-rust/fingerprint/3");
    let changed: Vec<_> = old
        .features
        .iter()
        .zip(&new.features)
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a.id.as_str())
        .collect();
    assert_eq!(changed, ["holes/1", "holes/2", "slots/1"]);
    let old_claims: BTreeSet<_> = old.features.iter().flat_map(|f| &f.faces).collect();
    assert_eq!(old_claims.len(), 86); // main: feature 86, body 0, unexplained 31
    assert_eq!(old.faces, new.faces);
    assert_eq!(old.features.len(), new.features.len());
    let correspondence = correspondence::correspond(&old, new).unwrap();
    for id in changed {
        let entry = correspondence
            .features
            .iter()
            .find(|f| f.old == id)
            .unwrap();
        assert_eq!(entry.new.as_deref(), Some(id), "{entry:?}");
        assert!(entry.candidates.is_empty());
    }
    let (sz, cz) = 17_f64.to_radians().sin_cos();
    let (sx, cx) = 23_f64.to_radians().sin_cos();
    let motion = [
        [cz, -sz, 0., 137.],
        [cx * sz, cx * cz, -sx, -83.],
        [sx * sz, sx * cz, cx, 49.],
    ];
    let moved = part.transformed(&motion).unwrap();
    let moved = quiddity::correspondence::recognise(&moved);
    let moved_doc = quiddity::recognition::document(&moved, moved.fingerprints.faces.len());
    assert_eq!(moved_doc.face_accounting, doc.face_accounting);
}
