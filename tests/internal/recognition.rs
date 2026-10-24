//! The versioned recognition document (`quiddity::recognition`, review finding M5): every
//! `Features` family has Python's record type, records are in `Features`' family order with
//! ascending faces within the part, and the document round-trips through JSON and is the same
//! from run to run.
//!
//! The document is the default recognition: the part recognised in its own frame and reported
//! in the file's coordinates (review M8). Over the corpus it must not depend on the part's
//! placement, and on parts aligned with the file's axes it must say what caller-space
//! recognition says; see [`the_default_document_moves_with_the_part`] and
//! [`the_default_document_agrees_with_caller_space_recognition`].

use super::common;
use super::slices;
#[path = "../support/published_geometry.rs"]
mod published_geometry;
#[path = "../support/recess_samples.rs"]
mod recess_samples;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quiddity::correspondence::{self, Recognition};
use quiddity::features::section_recess_family::recognise_section_recesses;
use quiddity::features::{self, reconcile};
use quiddity::framed_records::{self, Rigid, map_features};
use quiddity::frames::{self, PartFrame, RecordFrame};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use quiddity::published_records::PublishedFeatures;
use quiddity::recognition::{self, RECORD_TYPES, RecognitionDocument, families, record_types};
use quiddity::serve;
use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    common::fixtures().join(name)
}

/// The default recognition of the STEP file at *path* under *placement*, with its document.
fn default(path: &Path, placement: &Placement) -> (Recognition, RecognitionDocument) {
    let (part, recognition, _) = serve::recognise_step(&path.to_string_lossy(), placement).unwrap();
    let document = recognition::document(&recognition, part.faces.len());
    (recognition, document)
}

/// The document of the STEP file at *path*.
fn document(path: &Path) -> RecognitionDocument {
    default(path, &IDENTITY).1
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
    let table: Vec<&str> = RECORD_TYPES
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| *f != "modelled_threads")
        .collect();
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
    let problems: Vec<String> = common::parallel::map(&paths, |path| {
        let mut out = Vec::new();
        let name = path.file_name().unwrap().to_string_lossy();
        if let Ok((part, recognition, _)) =
            serve::recognise_step(&path.to_string_lossy(), &IDENTITY)
        {
            let doc = recognition::document(&recognition, part.faces.len());
            check(&name, &doc, part.faces.len(), &mut out);
            check_labels(&name, &doc, &mut out);
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// What is wrong with *doc*, the document of a part of *face_count* faces.
fn check(name: &str, doc: &RecognitionDocument, face_count: usize, out: &mut Vec<String>) {
    // Schema 5 makes complete face accounting mandatory for every new document.
    let version = recognition::SCHEMA_VERSION;
    if doc.schema_version != version || doc.face_count != face_count {
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

/// A part whose frame is the file's axes up to order and sign is reported wholly in file
/// coordinates; one turned 37° off them (`nonprincipal_37.step`, an orthogonal-gauge box) keeps
/// its axis letters in the frame, labelled, while its points and directions are the file's.
#[test]
fn fields_without_a_file_axis_publish_vectors() {
    let (_, aligned) = default(&fixture("filleted_plate_with_holes.step"), &IDENTITY);
    assert!(matches!(aligned.frame, RecordFrame::Inferred(_)));
    assert!(
        aligned
            .records
            .iter()
            .filter(|r| r.family == "fillets")
            .all(|r| r.parameters["axis"].is_string())
    );
    let (recognition, turned) = default(&fixture("nonprincipal_37.step"), &IDENTITY);
    let RecordFrame::Inferred(frame) = turned.frame else {
        panic!("no frame: {:?}", turned.frame);
    };
    let fillets: Vec<_> = turned
        .records
        .iter()
        .filter(|r| r.family == "fillets")
        .collect();
    assert!(!fillets.is_empty());
    for r in &fillets {
        assert!(r.parameters["axis"].is_array(), "{}", r.id);
    }
    // A blend along a fillet runs along the fillet's frame axis, in file coordinates.
    // Reconciliation rejects each blend for its fillet, so they are the working part's
    // candidates, carried into the file's coordinates as the default carries its records.
    let axis: [f64; 3] = serde_json::from_value(fillets[0].parameters["axis"].clone()).unwrap();
    let path = fixture("nonprincipal_37.step");
    let read = |p: &Placement| read_step_file_placed(&path, p);
    let part = read(&IDENTITY).unwrap();
    let working = frames::working_part(&read, &IDENTITY, &part, &frame).unwrap();
    let physical = features::inventory(&working).unwrap().physical;
    assert!(recognition.features.blends.is_empty());
    let blends = framed_records::to_file(physical, &frame, &working)
        .features
        .blends
        .clone();
    assert!(blends.iter().any(|b| match &b.path {
        quiddity::BlendPath::Straight(s) =>
            (0..3).all(|i| (s.direction[i].abs() - axis[i].abs()).abs() < 1e-6),
        _ => false,
    }));
    // The fingerprints read the letter as that frame axis.
    let print = recognition
        .fingerprints
        .features
        .iter()
        .find(|f| f.id == fillets[0].id)
        .unwrap();
    assert_eq!(print.axis, Some(axis));
}

/// What is wrong with the frame and labels of *doc*: a label only where a frame was inferred,
/// and each naming a parameter the record has.
fn check_labels(name: &str, doc: &RecognitionDocument, out: &mut Vec<String>) {
    fn scan(v: &Value, path: &str, out: &mut Vec<String>) {
        match v {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "local" {
                        out.push(format!("{path}: published local key"));
                    }
                    scan(value, &format!("{path}.{key}"), out);
                }
            }
            Value::Array(items) => {
                for value in items {
                    scan(value, path, out);
                }
            }
            _ => {}
        }
    }
    scan(&serde_json::to_value(doc).unwrap(), name, out);
}

/// The motions of `tests/invariance.rs`: a non-round translation and right-angle rotations with
/// exact ±1/0 entries (so the placement adds no round-off).
const T: [f64; 3] = [123.456, -78.9, 41.3];
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);

const MOTIONS: [Motion; 6] = [
    (
        "translate",
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        T,
    ),
    (
        "rot_z90",
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0; 3],
    ),
    (
        "rot_x90",
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_y180",
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0; 3],
    ),
    (
        "cycle_xyz",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_zx_moved",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        T,
    ),
];

/// Review M8's generic rotation, as `tests/invariance.rs`'s `generic_framed`: 37° about
/// (1, 2, 3), then the translation `T`.
const GENERIC: &str = "generic_framed";

/// The rotation by *degrees* about *axis* (Rodrigues), then *translation*.
fn rotation(axis: [f64; 3], degrees: f64, translation: [f64; 3]) -> Placement {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = axis.map(|c| c / n);
    let (s, c) = degrees.to_radians().sin_cos();
    let t = 1.0 - c;
    [
        [
            t * x * x + c,
            t * x * y - s * z,
            t * x * z + s * y,
            translation[0],
        ],
        [
            t * x * y + s * z,
            t * y * y + c,
            t * y * z - s * x,
            translation[1],
        ],
        [
            t * x * z - s * y,
            t * y * z + s * x,
            t * z * z + c,
            translation[2],
        ],
    ]
}

fn corpus_files() -> Vec<String> {
    common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_string())
        .collect()
}

/// A record as compared: its family, faces, parameters (as some mapping left them) and the
/// paths left in the frame.
struct Compared {
    family: String,
    faces: Vec<usize>,
    parameters: Value,
}

/// *doc*'s records with the parameters *features* gives them (the document's records carried
/// through some motion), reconstructed using their published measuring bases.
fn compared(doc: &RecognitionDocument, features: &PublishedFeatures) -> Vec<Compared> {
    let by_family: BTreeMap<String, Vec<Value>> = features.families().into_iter().collect();
    doc.records
        .iter()
        .map(|r| {
            let n: usize = r.id.rsplit('/').next().unwrap().parse().unwrap();
            Compared {
                family: r.family.clone(),
                faces: r.faces.clone(),
                parameters: {
                    let mut record = by_family[&r.family][n].clone();
                    if r.family == "paired_ramp_steps"
                        && !record["half_widths"].is_null()
                        && record.get("width_axis").is_none()
                    {
                        record["width_axis"] =
                            serde_json::json!(features.paired_ramp_steps[n].half_width_axis);
                    }
                    published_geometry::canonical(&record, &r.family)
                },
            }
        })
        .collect()
}

/// Numbers agree within one part in a million (of the larger, or of 1), as the parity tests
/// compare; a number that differs by no more than [`GRID`] is round-off at a published grid,
/// counted in *grid*. Anything else must be equal.
fn agree(a: &Value, b: &Value, grid: &mut usize) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            let d = (x - y).abs();
            if d <= 1e-6 * x.abs().max(y.abs()).max(1.0) {
                true
            } else if d <= GRID {
                *grid += 1;
                true
            } else {
                false
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| agree(p, q, grid))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| agree(v, w, grid)))
        }
        _ => a == b,
    }
}

/// The leaves of *a* and *b* that do not [`agree`], as `path: a | b`.
fn disagreeing(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                disagreeing(p, q, &format!("{path}[{i}]"), out);
            }
        }
        (Value::Object(x), Value::Object(y)) if x.len() == y.len() => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => disagreeing(v, w, &format!("{path}.{k}"), out),
                    None => out.push(format!("{path}.{k}: {v} | (none)")),
                }
            }
        }
        _ => {
            if !agree(a, b, &mut 0) {
                out.push(format!("{path}: {a} | {b}"));
            }
        }
    }
}

fn disagreement(a: &Value, b: &Value) -> String {
    let mut out = Vec::new();
    disagreeing(a, b, "", &mut out);
    out.join(", ")
}

/// One step of the coarsest grid a record publishes on (two decimals: a pattern's pitches and
/// angles, a channel's ends), plus round-off: a value rounded in one placement can land one step
/// from the other's.
const GRID: f64 = 0.01 + 1e-9;

/// A family whose records differ: how many, and how.
struct Difference {
    family: String,
    count: usize,
    text: String,
}

/// The families whose records differ between *expected* and *got*, matched by their faces
/// (records on equal faces paired in order), with every published measurement compared geometrically.
fn differences(expected: &[Compared], got: &[Compared], grid: &mut usize) -> Vec<Difference> {
    let mut families: Vec<&str> = expected
        .iter()
        .chain(got)
        .map(|r| r.family.as_str())
        .collect();
    families.dedup();
    families.sort_unstable();
    families.dedup();
    let mut out = Vec::new();
    for family in families {
        let indices = |list: &[Compared]| -> Vec<usize> {
            (0..list.len())
                .filter(|&i| list[i].family == family)
                .collect()
        };
        let (mut left, right) = (indices(expected), indices(got));
        let mut notes = Vec::new();
        let mut unmatched = Vec::new();
        for j in right {
            let Some(k) = left.iter().position(|&i| expected[i].faces == got[j].faces) else {
                unmatched.push(format!("+{:?}", got[j].faces));
                continue;
            };
            let i = left.remove(k);
            let (a, b) = (&expected[i], &got[j]);
            if !agree(&a.parameters, &b.parameters, grid) {
                notes.push(format!(
                    "{:?}: {}",
                    a.faces,
                    disagreement(&a.parameters, &b.parameters)
                ));
            }
        }
        unmatched.extend(left.iter().map(|&i| format!("-{:?}", expected[i].faces)));
        let count = unmatched.len() + notes.len();
        if count > 0 {
            unmatched.extend(notes);
            out.push(Difference {
                family: family.to_string(),
                count,
                text: unmatched.join("; "),
            });
        }
    }
    out
}

/// *frame* carried by *motion*.
fn moved_frame(frame: &PartFrame, motion: &Rigid) -> PartFrame {
    PartFrame {
        origin: motion.point(frame.origin),
        x: motion.direction(frame.x),
        y: motion.direction(frame.y),
        z: motion.direction(frame.z),
        gauge: frame.gauge,
    }
}

fn frames_agree(a: &PartFrame, b: &PartFrame) -> bool {
    let close = |p: [f64; 3], q: [f64; 3], tol: f64| (0..3).all(|i| (p[i] - q[i]).abs() <= tol);
    let scale = a.origin.iter().fold(1.0f64, |m, c| m.max(c.abs()));
    a.gauge == b.gauge
        && close(a.origin, b.origin, 1e-6 * scale)
        && close(a.x, b.x, 1e-9)
        && close(a.y, b.y, 1e-9)
        && close(a.z, b.z, 1e-9)
}

/// How the default recognition of a part placed by *motion* differs from its unmoved one, once
/// carried back by the motion's inverse.
fn under_motion(
    unmoved: &(Recognition, RecognitionDocument),
    moved: &(Recognition, RecognitionDocument),
    motion: &Rigid,
    grid: &mut usize,
) -> Vec<Difference> {
    let ((r0, d0), (r1, d1)) = (unmoved, moved);
    let back = map_features(r1.features.clone(), &motion.inverse());
    let expected = compared(d0, &r0.features);
    let got = compared(d1, &back.features);
    match (&r0.frame, &r1.frame) {
        (RecordFrame::Inferred(f0), RecordFrame::Inferred(f1)) => {
            // Where the gauge leaves the frame free (a sign, an interchange, a roll about the
            // axis), the moved part's frame can be another representative: its records are then
            // compared all the same, and fields the freedom moves differ.
            let mut found = differences(&expected, &got, grid);
            if !frames_agree(&moved_frame(f0, motion), f1) {
                for d in &mut found {
                    d.text = format!("(frames differ: {:?} gauge) {}", f0.gauge, d.text);
                }
            }
            found
        }
        (RecordFrame::Refused { reason: a }, RecordFrame::Refused { reason: b }) if a == b => {
            differences(&expected, &got, grid)
        }
        (a, b) => vec![Difference {
            family: "frame".into(),
            count: 1,
            text: format!("unmoved {a:?}, moved {b:?}"),
        }],
    }
}

/// `references/framed_document.json`'s entries, keyed by (file, check, family), with the most
/// records each may differ by.
fn known_entries() -> Vec<((String, String, String), usize)> {
    let known = common::load("references/framed_document.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("references/framed_document.json", known);
    known
        .iter()
        .map(|k| {
            let s = |f: &str| k[f].as_str().unwrap().to_string();
            let most = k["max_differing"]
                .as_u64()
                .unwrap_or_else(|| panic!("an entry gives max_differing: {k}"));
            ((s("file"), s("check"), s("family")), most as usize)
        })
        .collect()
}

/// The differences not explained by a listed entry of these *checks*, a listed entry exceeded,
/// and every listed entry of them whose file is *owned* (the slice's, `tests/support/slices.rs`)
/// no longer seen.
fn unexplained(
    checks: &[&str],
    owned: impl Fn(&str) -> bool,
    found: impl Iterator<Item = (String, String, Vec<Difference>)>,
) -> Vec<String> {
    let known: Vec<_> = known_entries()
        .into_iter()
        .filter(|((file, check, _), _)| checks.contains(&check.as_str()) && owned(file))
        .collect();
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for (file, check, differences) in found {
        for d in differences {
            let key = (file.clone(), check.clone(), d.family.clone());
            match known.iter().find(|k| k.0 == key) {
                Some((_, most)) => {
                    if d.count > *most {
                        problems.push(format!(
                            "{file} {check} {} is listed with at most {most} differing, now {}: {}",
                            d.family, d.count, d.text
                        ));
                    }
                    seen.push(key);
                }
                None => problems.push(format!(
                    "{file} {check} {}: {} differing, {}",
                    d.family, d.count, d.text
                )),
            }
        }
    }
    for (key, most) in &known {
        // Zero is an absence ratchet for a resolved historical discrepancy. Any return is
        // still rejected above; retaining the entry does not permit a difference.
        if *most != 0 && !seen.contains(key) {
            problems.push(format!(
                "{key:?} is listed but no longer differs: remove it"
            ));
        }
    }
    problems
}

/// Every corpus part's default document, read unmoved, under `tests/invariance.rs`'s motions and
/// under the generic rotation, and carried back by the motion's inverse, must find the same
/// features on the same faces, with every parameter in the file's coordinates agreeing (to one
/// part in a million, or within one step of a two-decimal grid, which is round-off rounded in one
/// placement and not the other). A parameter left in the frame (a letter, a coordinate along a
/// frame axis) is compared in the frame: the moved part's frame must be the unmoved one moved, and
/// the records carried into it must agree. Differences are listed in
/// `tests/fixtures/references/framed_document.json` under the motion's name.
///
/// One test per slice of the corpus (`tests/support/slices.rs`); the grid count printed is the
/// slice's.
fn moves_with_the_part(k: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let mut motions: Vec<(&str, Placement)> = MOTIONS
        .iter()
        .map(|(name, r, t)| (*name, [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]])))
        .collect();
    motions.push((GENERIC, rotation([1.0, 2.0, 3.0], 37.0, T)));
    let all = corpus_files();
    let files = slices::slice(&all, k, n);
    // Every part's default recognition unmoved and under every motion, all spread over every
    // core at once (so a costly part's readings run side by side).
    let readings: Vec<(usize, &Placement)> = (0..files.len())
        .flat_map(|i| {
            std::iter::once(&IDENTITY)
                .chain(motions.iter().map(|(_, p)| p))
                .map(move |p| (i, p))
        })
        .collect();
    let mut read = common::parallel::map(&readings, |&(i, placement)| {
        let path = dir.join(&files[i]);
        serve::recognise_step(&path.to_string_lossy(), placement).map(|(part, recognition, _)| {
            super::vector_publication::validate_boxes(
                &serde_json::to_value(&recognition.features).unwrap(),
                &part,
            );
            let document = recognition::document(&recognition, part.faces.len());
            (recognition, document)
        })
    })
    .into_iter();
    let mut grid = 0;
    let mut found: Vec<(String, String, Vec<Difference>)> = Vec::new();
    for name in &files {
        let unmoved = read.next().unwrap();
        let moved: Vec<_> = read.by_ref().take(motions.len()).collect();
        // The corpus test reports read failures.
        let Ok(unmoved) = unmoved else {
            continue;
        };
        for ((motion, placement), moved) in motions.iter().zip(moved) {
            let d = under_motion(
                &unmoved,
                &moved.unwrap(),
                &Rigid::from_placement(placement),
                &mut grid,
            );
            found.push((name.clone(), motion.to_string(), d));
        }
    }
    eprintln!("values agreeing only to the two-decimal grid: {grid}");
    let checks: Vec<&str> = motions.iter().map(|m| m.0).collect();
    let owned = |file: &str| slices::owns(&all, k, n, file);
    let problems = unexplained(&checks, owned, found.into_iter());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    the_default_document_moves_with_the_part,
    super::moves_with_the_part,
    [
        0 => slice_0,
        1 => slice_1,
        2 => slice_2,
        3 => slice_3,
        4 => slice_4,
        5 => slice_5,
        6 => slice_6,
        7 => slice_7,
        8 => slice_8,
        9 => slice_9,
        10 => slice_10,
        11 => slice_11,
    ]
);

/// The check name of [`the_default_document_agrees_with_caller_space_recognition`].
const CALLER_SPACE: &str = "caller_space";

/// On every corpus part, the default document against caller-space recognition
/// (`correspondence::caller_space_recognise`, Python's entry points' space) of the part as read: the same
/// features on the same faces, every parameter in the file's coordinates agreeing as
/// [`the_default_document_moves_with_the_part`] compares them. A parameter the default leaves in
/// the frame (a part whose frame is not the file's axes up to order and sign) has no file-space
/// value to compare. Every difference is listed in `tests/fixtures/references/framed_document.json`
/// under `caller_space`, with a verdict.
///
/// One test per slice of the corpus (`tests/support/slices.rs`); the counts printed are the
/// slice's.
fn agrees_with_caller_space(k: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let all = corpus_files();
    let files = slices::slice(&all, k, n);
    let found = common::parallel::map(&files, |name| {
        let path = dir.join(name);
        let Ok((part, framed, _)) = serve::recognise_step(&path.to_string_lossy(), &IDENTITY)
        else {
            return (Vec::new(), 0, 0);
        };
        let caller = correspondence::caller_space_recognise(&part);
        for features in [&caller.features, &framed.features] {
            super::vector_publication::validate_boxes(
                &serde_json::to_value(features).unwrap(),
                &part,
            );
        }
        let known = common::load("references/framed_document.json");
        if known.as_array().unwrap().iter().any(|v| {
            v["file"] == *name
                && v["check"] == CALLER_SPACE
                && v["family"] == "section_recesses"
                && v["verdict"] == "equivalent"
        }) {
            recess_samples::check(name, &caller, &framed);
        }
        let (df, dc) = (
            recognition::document(&framed, part.faces.len()),
            recognition::document(&caller, part.faces.len()),
        );
        let relative = df
            .records
            .iter()
            .filter(|r| r.parameters.contains_key("origin"))
            .count();
        let mut grid = 0;
        let d = differences(
            &compared(&dc, &caller.features),
            &compared(&df, &framed.features),
            &mut grid,
        );
        (d, grid, relative)
    });
    let (mut grid, mut relative) = (0, 0);
    let found: Vec<(String, String, Vec<Difference>)> = files
        .iter()
        .zip(found)
        .map(|(name, (d, g, l))| {
            grid += g;
            relative += l;
            (name.clone(), CALLER_SPACE.to_string(), d)
        })
        .collect();
    eprintln!(
        "against caller space: {grid} values agree only to the two-decimal grid; {relative} records \
         use an explicit measurement origin"
    );
    let owned = |file: &str| slices::owns(&all, k, n, file);
    let problems = unexplained(&[CALLER_SPACE], owned, found.into_iter());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    the_default_document_agrees_with_caller_space_recognition,
    super::agrees_with_caller_space,
    [
        0 => slice_0,
        1 => slice_1,
        2 => slice_2,
        3 => slice_3,
        4 => slice_4,
        5 => slice_5,
    ]
);

/// Corpus parts on which reconciliation rejects candidates, between them by every rule that
/// rejects a carried family and that the corpus reaches (`captured/reconcile/capture.json.gz`).
const RECONCILED: [&str; 8] = [
    "cadgenbench/flanged_spool_132.step",
    "cadgenbench_inputs/cgb207.step",
    "mfcadpp/1000.step",
    "mfcadpp/10000.step",
    "mfcadpp/10190.step",
    "mfcadpp/10245.step",
    "mfcadpp_holdout/467.step",
    "nist/nist_ctc_03_asme1_rc.stp",
];

/// Whether *needle* is *v* or inside it.
fn contains(v: &Value, needle: &Value) -> bool {
    v == needle
        || match v {
            Value::Array(items) => items.iter().any(|x| contains(x, needle)),
            Value::Object(fields) => fields.values().any(|x| contains(x, needle)),
            _ => false,
        }
}

fn sorted(faces: &[usize]) -> Vec<usize> {
    let mut faces = faces.to_vec();
    faces.sort_unstable();
    faces
}

/// The `Features` field of a family a rule reads.
fn family_field(family: reconcile::Family) -> Option<&'static str> {
    match family {
        reconcile::Family::Passages => Some("section_passages"),
        other => Some(
            RECORD_TYPES
                .iter()
                .map(|(f, _)| *f)
                .find(|f| *f == other.value())
                .expect("a carried family"),
        ),
    }
}

/// Each rejected candidate of a carried family: its field, its defining faces (ascending) and
/// how many of the family's candidates have those faces.
fn rejected(inventory: &features::Inventory) -> Vec<(&'static str, Vec<usize>, usize)> {
    inventory
        .dispositions
        .iter()
        .filter(|d| d.outcome == reconcile::Outcome::Rejected)
        .filter_map(|d| {
            let field = family_field(d.candidate.family)?;
            let faces = d.candidate.defining.clone();
            let same = inventory.physical.defining[field]
                .iter()
                .filter(|f| sorted(f) == faces)
                .count();
            Some((field, faces, same))
        })
        .collect()
}

/// A candidate reconciliation rejects is in neither `features::recognise`'s records nor the
/// recognition document: on parts where each rejecting rule the corpus reaches fires, each
/// rejected candidate's defining faces are held by as many fewer records of its family as were
/// rejected with them (by none, unless an accepted candidate has the same faces), in both, and
/// each family has exactly as many fewer records as it had rejected candidates.
#[test]
fn rejected_candidates_are_absent_from_recognition_and_the_document() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        return;
    };
    let found = common::parallel::map(&RECONCILED, |file| {
        let part = quiddity::read_step_file(&dir.join(file)).unwrap();
        let inventory = features::inventory(&part).unwrap();
        let rejected = rejected(&inventory);
        let recognition = correspondence::caller_space_recognise(&part);
        let document = recognition::document(&recognition, part.faces.len());
        if *file == "mfcadpp/10000.step" {
            // #89: this inward-facing round used to reach reconciliation as a false
            // convex fillet. Reject it at discovery, retaining its circular-step reading.
            assert_eq!(part.frame_points_outward(10), Some(false));
            assert!(!inventory.physical.defining["fillets"].contains(&vec![10]));
            assert!(
                document
                    .records
                    .iter()
                    .any(|r| { r.family == "circular_blind_steps" && r.faces == vec![9, 10] })
            );
        }
        let reasons: Vec<&str> = inventory
            .dispositions
            .iter()
            .filter(|d| d.outcome == reconcile::Outcome::Rejected)
            .map(|d| d.reason.value())
            .collect();
        let mut problems = Vec::new();
        for (field, faces, same) in &rejected {
            let lost = rejected
                .iter()
                .filter(|(f, g, _)| f == field && g == faces)
                .count();
            let kept = recognition.features.defining[field]
                .iter()
                .filter(|f| sorted(f) == *faces)
                .count();
            let in_document = document
                .records
                .iter()
                .filter(|r| r.family == *field && r.faces == *faces)
                .count();
            if kept != same - lost || in_document != same - lost {
                problems.push(format!(
                    "{file}: rejected {field} {faces:?} is kept {kept} times and in the document \
                     {in_document} times ({same} candidates, {lost} rejected)"
                ));
            }
        }
        for (field, candidates) in &inventory.physical.defining {
            let lost = rejected.iter().filter(|(f, ..)| f == field).count();
            let records = document
                .records
                .iter()
                .filter(|r| r.family == *field)
                .count();
            if records != candidates.len() - lost {
                problems.push(format!(
                    "{file}: {field} has {records} records of {} candidates, {lost} rejected",
                    candidates.len()
                ));
            }
        }
        (problems, reasons, rejected.len())
    });
    let mut problems = Vec::new();
    let mut reasons: Vec<&str> = Vec::new();
    for (file, (p, r, n)) in RECONCILED.iter().zip(found) {
        assert!(n > 0, "{file}: reconciliation rejects nothing");
        problems.extend(p);
        reasons.extend(r);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    reasons.sort_unstable();
    reasons.dedup();
    // Every rejecting rule but those only the synthetic scenarios reach
    // (`tests/reconcile.rs`).
    let unreached = [
        // #89 removes the concave false candidates that formerly reached this rule.
        // reconcile::rust_snapshot_reconcile_scenarios still exercises it explicitly.
        "blend.fillet_superseded_by_circular_blind_step",
        "bore.hole_superseded_by_double_d_bore",
        "recess.pocket_superseded_by_edge_open_circular_pocket",
        "recess.pocket_superseded_by_prismatic",
        "recess.slot_superseded_by_prismatic",
    ];
    let mut want: Vec<&str> = reconcile::ReasonCode::ALL
        .iter()
        .filter(|r| r.spec().0 == reconcile::Outcome::Rejected)
        .map(|r| r.value())
        .filter(|r| !unreached.contains(r))
        .collect();
    want.sort_unstable();
    assert_eq!(reasons, want);
}

/// Patterns are derived from accepted members only (`_derive_patterns`): rejecting a pocket of
/// a pocket pattern, or a hole of a hole pattern, takes it out of every pattern. No corpus part
/// has a pattern with a member reconciliation rejects, so a rejection is added to a real run's
/// decisions.
#[test]
fn a_pattern_never_includes_a_rejected_member() {
    for (name, field, patterns, family, reason, winner) in [
        (
            "golden_blind_pockets_and_pocket_patterns.step",
            "pockets",
            "pocket_patterns",
            reconcile::Family::Pockets,
            reconcile::ReasonCode::PocketSupersededByPassage,
            reconcile::Family::Passages,
        ),
        (
            "golden_bolt_circle_and_rectangular_grid.step",
            "holes",
            "hole_patterns",
            reconcile::Family::Holes,
            reconcile::ReasonCode::HoleSupersededByDoubleDBore,
            reconcile::Family::DoubleDBores,
        ),
    ] {
        let part = quiddity::read_step_file(&fixture(name)).unwrap();
        let mut inventory = features::inventory(&part).unwrap();
        let before = serde_json::to_value(inventory.clone().accepted()).unwrap();
        let members = before[field].as_array().unwrap().clone();
        // The first record that is a pattern's member, and one record.
        let at = members
            .iter()
            .position(|m| contains(&before[patterns], m))
            .unwrap_or_else(|| panic!("{name}: no {field} pattern"));
        let member = members[at].clone();
        assert_eq!(members.iter().filter(|m| **m == member).count(), 1);
        inventory.dispositions.push(reconcile::Disposition {
            candidate: reconcile::Candidate {
                family,
                index: at,
                defining: sorted(&inventory.physical.defining[field][at]),
            },
            outcome: reconcile::Outcome::Rejected,
            reason,
            related: vec![reconcile::Candidate {
                family: winner,
                index: 0,
                defining: Vec::new(),
            }],
        });
        let after = inventory.accepted();
        let json = serde_json::to_value(&after).unwrap();
        assert_eq!(json[field].as_array().unwrap().len(), members.len() - 1);
        assert_eq!(after.defining[field].len(), members.len() - 1);
        assert!(
            !contains(&json[patterns], &member),
            "{name}: a {patterns} record still has the rejected member"
        );
    }
}

/// On every corpus part `features::recognise` (through `correspondence::caller_space_recognise`) carries the
/// section recesses `recognise_section_recesses` gives, or its refusal, each defined by its
/// record's own defining faces and fingerprinted; the default (framed) recognition fingerprints
/// those its run finds, and the document publishes every one with its file-space geometry,
/// defining faces and self-contained coordinates. One test per slice of the corpus (`tests/support/slices.rs`).
fn carries_section_recesses(k: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        return;
    };
    let files = slices::slice(&corpus_files(), k, n);
    let problems = common::parallel::map(&files, |file| {
        let part = quiddity::read_step_file(&dir.join(file)).unwrap();
        let recognition = correspondence::caller_space_recognise(&part);
        let features = &recognition.features;
        let mut out = Vec::new();
        match recognise_section_recesses(&part) {
            Ok(records) => {
                if features.section_recesses != records || features.section_recess_refusal.is_some()
                {
                    out.push(format!("{file}: recognise's section recesses differ"));
                }
            }
            Err(e) => {
                if features.section_recess_refusal.as_ref() != Some(&e)
                    || !features.section_recesses.is_empty()
                {
                    out.push(format!("{file}: recognise does not carry the refusal {e}"));
                }
            }
        }
        let defining = &features.defining["section_recesses"];
        if defining.len() != features.section_recesses.len() {
            out.push(format!(
                "{file}: not one defining-face list per section recess"
            ));
        }
        for (i, r) in features.section_recesses.iter().enumerate() {
            let id = format!("section_recesses/{i}");
            if defining
                .get(i)
                .is_none_or(|d| d != r.evidence().defining_faces() || d.is_empty())
            {
                out.push(format!(
                    "{file}: {id}'s defining faces are not its record's"
                ));
            }
            let print = recognition
                .fingerprints
                .features
                .iter()
                .find(|f| f.id == id);
            if !print.is_some_and(|f| !f.faces.is_empty() && !f.sizes.is_empty()) {
                out.push(format!(
                    "{file}: {id} has no fingerprint with faces and sizes"
                ));
            }
        }
        let (framed, doc) = default(&dir.join(file), &IDENTITY);
        let printed = framed
            .fingerprints
            .features
            .iter()
            .filter(|f| f.family == "section_recesses" && !f.faces.is_empty())
            .count();
        if printed != framed.features.section_recesses.len() {
            out.push(format!(
                "{file}: the framed section recesses are not all fingerprinted"
            ));
        }
        let published: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "section_recesses" && r.record_type == "SectionRecess")
            .collect();
        if published.len() != framed.features.section_recesses.len() {
            out.push(format!("{file}: the document loses section recesses"));
        }
        for (i, (record, recess)) in published
            .iter()
            .zip(&framed.features.section_recesses)
            .enumerate()
        {
            let id = format!("section_recesses/{i}");
            if record.id != id
                || record.record_type != "SectionRecess"
                || record.faces != recess.evidence().defining_faces()
                || Value::Object(record.parameters.clone()) != serde_json::to_value(recess).unwrap()
            {
                out.push(format!("{file}: {id} is not published faithfully"));
            }
        }
        let refusals: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "section_recesses" && r.record_type == "SectionRecessRefusal")
            .collect();
        if refusals.len() != framed.features.section_recess_refusals.len() {
            out.push(format!(
                "{file}: the document loses section recess refusals"
            ));
        }
        for (i, (record, refusal)) in refusals
            .iter()
            .zip(&framed.features.section_recess_refusals)
            .enumerate()
        {
            let id = format!(
                "section_recesses/{}",
                framed.features.section_recesses.len() + i
            );
            if record.id != id
                || record.faces != refusal.evidence().defining_faces()
                || Value::Object(record.parameters.clone())
                    != serde_json::to_value(refusal).unwrap()
            {
                out.push(format!("{file}: {id} refusal is not published faithfully"));
            }
        }
        out
    });
    let problems: Vec<String> = problems.into_iter().flatten().collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    recognition_carries_section_recesses,
    super::carries_section_recesses,
    [
        0 => slice_0,
        1 => slice_1,
        2 => slice_2,
        3 => slice_3,
    ]
);

/// A sampling refusal retains the cavity's source anatomy and volume estimate in the public
/// document, without turning an unproved widening into a fingerprinted feature.
#[test]
fn a_void_without_a_proved_throat_retains_its_measurements() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = quiddity::read_step_file(&dir.join("mfcadpp_regressions/11512.step")).unwrap();
    let rec = correspondence::caller_space_recognise(&part);
    let refused = rec
        .features
        .interior_void_refusals
        .iter()
        .find(|r| r.candidate.void_faces == [33, 34])
        .expect("retained bounded-air candidate");
    assert_eq!(refused.reason, "widening_behind_throat_unproved");
    assert!(refused.candidate.estimated_volume > 0.0);
    assert!(refused.candidate.enclosed_samples > 0);
    assert!(!refused.candidate.openings.is_empty());
    let doc = recognition::document(&rec, part.faces.len());
    let record = doc
        .records
        .iter()
        .find(|r| r.record_type == "InteriorVoidRefusal" && r.faces == [33, 34])
        .unwrap();
    assert_eq!(
        record.evidence_status,
        quiddity::features::evidence::EvidenceStatus::Unproved
    );
    assert_eq!(
        serde_json::json!(record.parameters),
        serde_json::to_value(refused).unwrap()
    );
    assert!(!rec.fingerprints.features.iter().any(|f| f.id == record.id));
    assert_eq!(doc.schema_version, recognition::SCHEMA_VERSION);
    // A refusal explains a candidate that was not published; it is not an unproved feature.
    assert!(record.is_refusal());
    let path = dir.join("mfcadpp_regressions/11512.step");
    let (output, warnings) = serve::recognise_file(path.to_str().unwrap()).unwrap();
    let refusals: Vec<_> = output
        .document
        .records
        .iter()
        .filter(|r| r.record_type == "InteriorVoidRefusal")
        .map(|r| r.id.clone())
        .collect();
    assert!(!refusals.is_empty());
    assert!(
        warnings
            .iter()
            .all(|w| refusals.iter().all(|id| !w.contains(&format!("\"{id}\"")))),
        "{warnings:?}"
    );
}

/// Issue #76: on NIST CTC-04 each R5 pocket corner's 5x45° rim chamfer turns the corner on a
/// quarter-cone (or a narrower patch) that meets the corner-radius cylinder. Neither runs round
/// its axis, so it is no countersink; the corner faces stay accounted for. True countersinks,
/// whole cones over a bore, are kept.
#[test]
fn pocket_rim_chamfer_corners_are_not_countersinks() {
    use quiddity::kernel::geom::Surface;
    use quiddity::recognition::FaceAccounting;
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let path = dir.join("nist/nist_ctc_04_asme1_rd.stp");
    let part = quiddity::read_step_file(&path).unwrap();
    // The twelve corner cones: apex 10 above the pocket rim at each R5 corner.
    let xs = [
        -149.0, -128.4137, -104.5863, -84.0, 84.0, 104.5863, 128.4137, 149.0,
    ];
    let ys = [278.0, 337.1768, 398.0];
    let corners: Vec<usize> = (0..part.faces.len())
        .filter(|&f| {
            matches!(part.faces[f].surface, Surface::Cone { .. })
                && part.faces[f].surface.cone_apex().is_some_and(|a| {
                    (a[2] + 30.0).abs() < 1e-3
                        && xs.iter().any(|x| (a[0] - x).abs() < 1e-3)
                        && ys.iter().any(|y| (a[1] - y).abs() < 1e-3)
                })
        })
        .collect();
    assert_eq!(corners.len(), 12, "{corners:?}");
    let doc = document(&path);
    let count = |family: &str| doc.records.iter().filter(|r| r.family == family).count();
    assert_eq!(count("countersinks"), 0);
    // The straight runs of the rim chamfer are still recognised either side of each corner.
    assert_eq!((count("chamfers"), count("oriented_chamfers")), (8, 4));
    let accounting = doc.face_accounting.as_ref().unwrap();
    for &f in &corners {
        // No record spans the rim round its corners, so these faces are left for review.
        assert_eq!(accounting[f], FaceAccounting::Unexplained, "face {f}");
    }
    // Control: whole cones seated on bores remain countersinks.
    let seated = document(&fixture("captured/83644e6c09150015.step.gz"));
    assert_eq!(
        seated
            .records
            .iter()
            .filter(|r| r.family == "countersinks")
            .count(),
        3
    );
}

/// Issue #76's sweep rule keeps a seat that a neighbouring cut has trimmed. Fixture (build123d):
/// a 40x30x10 plate with a countersunk through hole (drill r 3, seat r 6, 90°) at its centre,
/// crossed by a 3 wide, 5 deep slot that splits the seat into two faces of about 151° each.
/// Together they run about 302° round the axis, over the 270° the rule asks.
#[test]
fn a_seat_split_by_a_slot_stays_a_countersink() {
    use quiddity::kernel::geom::Surface;
    use quiddity::recognition::FaceAccounting;
    let path = fixture("countersink_split_by_slot.step");
    let part = quiddity::read_step_file(&path).unwrap();
    let cones: Vec<usize> = (0..part.faces.len())
        .filter(|&f| matches!(part.faces[f].surface, Surface::Cone { .. }))
        .collect();
    assert_eq!(cones.len(), 2, "{cones:?}");
    let doc = document(&path);
    let accounting = doc.face_accounting.as_ref().unwrap();
    for &f in &cones {
        let FaceAccounting::Feature { record_ids } = &accounting[f] else {
            panic!("face {f}: {:?}", accounting[f]);
        };
        assert!(
            record_ids.iter().all(|id| id.starts_with("countersinks/")),
            "face {f}: {record_ids:?}"
        );
    }
}

/// #81: a different set of principal runs must not erase physical rounds. The other set
/// remains represented by exact cylindrical blends. Cross-drilled through holes also must
/// survive even when the six-axis chamber sampler correctly refuses its old coarse-grid claim.
#[test]
fn changed_query_axes_retain_physical_rounds_and_bores() {
    use quiddity::kernel::geom::{self, Surface};
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let part = quiddity::read_step_file(&dir.join("cadgenbench_inputs/cgb217.step.gz")).unwrap();
    let framed = correspondence::recognise(&part);
    let caller = correspondence::caller_space_recognise(&part);
    for (specific, general, faces) in [
        (&framed, &caller, [177, 182, 205, 210, 233, 238, 261, 266]),
        (&caller, &framed, [191, 196, 219, 224, 247, 252, 296, 301]),
    ] {
        for face in faces {
            let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
                panic!("{face}")
            };
            assert_eq!(radius, 1.);
            assert_eq!(part.frame_points_outward(face), Some(true), "{face}");
            let planes: Vec<_> = part
                .neighbours(face)
                .into_iter()
                .filter_map(|f| {
                    let Surface::Plane { frame: plane } = part.faces[f].surface else {
                        return None;
                    };
                    Some(plane)
                })
                .collect();
            assert_eq!(planes.len(), 2, "{face}");
            assert!(geom::dot(planes[0].z, planes[1].z).abs() < 1e-9, "{face}");
            for plane in planes {
                assert!(geom::dot(plane.z, frame.z).abs() < 1e-9, "{face}");
                assert!(
                    (geom::dot(geom::sub(frame.origin, plane.origin), plane.z).abs() - radius)
                        .abs()
                        < 1e-6,
                    "{face}"
                );
            }
            let index = specific.features.defining["fillets"]
                .iter()
                .position(|f| *f == vec![face])
                .unwrap();
            assert_eq!(specific.features.fillets[index].radius, radius);
            let index = general.features.defining["blends"]
                .iter()
                .position(|f| *f == vec![face])
                .unwrap();
            let blend = &general.features.blends[index];
            assert_eq!(blend.radius, radius);
            assert_eq!(blend.side, "convex");
            let features::blends::BlendPath::Straight(path) = &blend.path else {
                panic!("{face}")
            };
            assert!(
                geom::dot(geom::unit(path.direction).unwrap(), frame.z).abs() > 1. - 1e-9,
                "{face}"
            );
        }
    }
    let mut bores = Vec::new();
    for (face, radius) in [(26, 4.25), (28, 4.25), (41, 5.1)] {
        let Surface::Cylinder {
            frame,
            radius: measured,
        } = part.faces[face].surface
        else {
            panic!("{face}")
        };
        assert!((measured - radius).abs() < 1e-9);
        assert_eq!(part.frame_points_outward(face), Some(false));
        bores.push(frame.z);
        for reading in [&framed, &caller] {
            let index = reading.features.defining["holes"]
                .iter()
                .position(|f| *f == vec![face])
                .unwrap();
            assert!((reading.features.holes[index].diameter - 2. * radius).abs() < 1e-9);
            assert_eq!(
                reading.features.holes[index].bottom,
                features::holes::Bottom::Through
            );
        }
    }
    assert!(geom::dot(bores[0], bores[1]).abs() > 1. - 1e-9);
    assert!(geom::dot(bores[0], bores[2]).abs() < 1e-9);
    // Neither cross-bore junction establishes 1.5x widening relative to the continuous
    // R5.1 exit. The old caller grid accepted an incomplete two-wall sampled component.
    assert!((4.25_f64.powi(2) + 5.1_f64.powi(2)).sqrt() < 1.5 * 5.1);
    assert!(framed.features.interior_voids.is_empty());
    let document = recognition::document(&framed, part.faces.len());
    assert!(
        document
            .records
            .iter()
            .any(|r| r.record_type == "InteriorVoidRefusal" && r.faces == vec![26, 28, 41])
    );

    let part = quiddity::read_step_file(&dir.join("cadgenbench_inputs/cgb241.step")).unwrap();
    let framed = correspondence::recognise(&part);
    let caller = correspondence::caller_space_recognise(&part);
    for (face, radius, outward, side) in [(27, 20., false, "concave"), (47, 23., true, "convex")] {
        let Surface::Cylinder {
            radius: measured, ..
        } = part.faces[face].surface
        else {
            panic!("{face}")
        };
        assert_eq!(measured, radius);
        assert_eq!(part.frame_points_outward(face), Some(outward));
        let index = caller.features.defining["blends"]
            .iter()
            .position(|f| *f == vec![face])
            .unwrap();
        assert_eq!(caller.features.blends[index].side, side);
        // Permit a future correction of the approximate fillet label; require the curve
        // itself to remain represented with the right radius in either accepted family.
        let found = framed.features.defining["fillets"]
            .iter()
            .zip(&framed.features.fillets)
            .any(|(f, r)| *f == vec![face] && r.radius == radius)
            || framed.features.defining["blends"]
                .iter()
                .zip(&framed.features.blends)
                .any(|(f, r)| *f == vec![face] && r.radius == radius);
        assert!(found, "lost physical cylindrical blend {face}");
    }
    let normal = |f: usize| {
        let Surface::Plane { frame } = part.faces[f].surface else {
            panic!()
        };
        frame.z
    };
    assert!((geom::dot(normal(26), normal(28)) + 0.03589128343024717).abs() < 1e-12);
}

/// For #82: alpha.8 chose one of two overlapping, equal-support 49.5 mm rows by
/// input order. Under ADR 0011 neither is published: neither row owns the two central holes
/// uniquely. No refusal record is emitted; the holes themselves are still recognised.
#[test]
fn ctc02_refuses_the_ambiguous_49_5_hole_arrays() {
    use features::hole_patterns::HolePattern;
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let (recognised, _) = default(&dir.join("nist/nist_ctc_02_asme1_rc.stp"), &IDENTITY);
    let patterns = &recognised.features.hole_patterns;
    assert_eq!(patterns.len(), 13);
    let mut pitches: Vec<_> = patterns
        .iter()
        .filter_map(|p| match p {
            HolePattern::LinearArray { pitch, .. } => Some(*pitch),
            _ => None,
        })
        .collect();
    pitches.sort_by(f64::total_cmp);
    pitches.dedup();
    assert_eq!(pitches, [48., 49.76, 75.]);
    assert!(!patterns.iter().any(|p| matches!(p,
        HolePattern::LinearArray { pitch, holes, .. } if *pitch == 49.5 && holes.len() >= 3
    )));
    let mut stations: Vec<_> = recognised
        .features
        .holes
        .iter()
        .filter(|h| h.diameter == 8.38 && h.location[1] == 128. && h.location[2] == -360.)
        .map(|h| h.location[0])
        .collect();
    stations.sort_by(f64::total_cmp);
    assert_eq!(stations, [-122., -74., -25., 25., 74., 122.]);
    // Reflection exchanges these span-99 readings; both claim x = -25 and +25.
    assert_eq!((stations[3] - stations[1]) / 2., 49.5);
    assert_eq!((stations[4] - stations[2]) / 2., 49.5);
}

/// For #83: draftwright-rust's identical seats at opposite mouths remain attached.
#[test]
fn a_two_face_countersink_is_attached_to_its_bore() {
    let path = fixture("countersink_two_face.step.gz");
    let (recognised, _) = default(&path, &IDENTITY);
    let features = &recognised.features;
    assert_eq!(features.countersinks.len(), 2);
    let hole = features.holes.iter().find(|h| h.diameter == 6.).unwrap();
    let seat = hole.csink.as_ref().expect("the 6 mm bore has a seat");
    assert!(features.countersinks.contains(seat));
    assert_eq!(
        (
            seat.drill_diameter,
            seat.major_diameter,
            seat.included_angle
        ),
        (6., 14., 90.)
    );
}
