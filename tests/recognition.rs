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

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quiddity::correspondence::{self, Recognition};
use quiddity::features::{self, Features};
use quiddity::framed_records::{Rigid, map_features};
use quiddity::frames::{PartFrame, RecordFrame};
use quiddity::kernel::step::{IDENTITY, Placement};
use quiddity::recognition::{
    self, RECORD_TYPES, RecognitionDocument, SCHEMA_VERSION, families, record_types,
};
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
                        let Ok((part, recognition, _)) =
                            serve::recognise_step(&path.to_string_lossy(), &IDENTITY)
                        else {
                            continue;
                        };
                        let doc = recognition::document(&recognition, part.faces.len());
                        check(&name, &doc, part.faces.len(), &mut out);
                        check_labels(&name, &doc, &mut out);
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

/// A part whose frame is the file's axes up to order and sign is reported wholly in file
/// coordinates; one turned 37° off them (`nonprincipal_37.step`, an orthogonal-gauge box) keeps
/// its axis letters in the frame, labelled, while its points and directions are the file's.
#[test]
fn fields_without_a_file_axis_are_labelled() {
    let (_, aligned) = default(&fixture("filleted_plate_with_holes.step"), &IDENTITY);
    assert!(matches!(aligned.frame, RecordFrame::Inferred(_)));
    assert!(aligned.records.iter().all(|r| r.local.is_empty()));
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
        assert_eq!(r.local, ["axis"], "{}", r.id);
    }
    // A blend along a fillet runs along the fillet's frame axis, in file coordinates.
    let letter = fillets[0].parameters["axis"].as_str().unwrap();
    let axis = match letter {
        "x" => frame.x,
        "y" => frame.y,
        _ => frame.z,
    };
    let blends = &recognition.features.blends;
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
    let inferred = matches!(doc.frame, RecordFrame::Inferred(_));
    for r in &doc.records {
        if !inferred && !r.local.is_empty() {
            out.push(format!("{name}: {} has local fields without a frame", r.id));
        }
        let mut parameters = Value::Object(r.parameters.clone());
        for path in &r.local {
            if take(&mut parameters, path).is_empty() {
                out.push(format!(
                    "{name}: {} labels {path}, which it does not have",
                    r.id
                ));
            }
        }
    }
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
    local: Vec<String>,
}

/// *doc*'s records with the parameters *features* gives them (the document's records carried
/// through some motion), and *extra* local paths by record ID.
fn compared(
    doc: &RecognitionDocument,
    features: &Features,
    extra: &BTreeMap<String, Vec<String>>,
) -> Vec<Compared> {
    let by_family: BTreeMap<String, Vec<Value>> = families(features).into_iter().collect();
    doc.records
        .iter()
        .map(|r| {
            let n: usize = r.id.rsplit('/').next().unwrap().parse().unwrap();
            let mut local = r.local.clone();
            local.extend(extra.get(&r.id).into_iter().flatten().cloned());
            Compared {
                family: r.family.clone(),
                faces: r.faces.clone(),
                parameters: by_family[&r.family][n].clone(),
                local,
            }
        })
        .collect()
}

/// The values at *path* in *v* (`a.b` a nested field, `a[]` each item of a list), removed.
fn take(v: &mut Value, path: &str) -> Vec<Value> {
    let (head, rest) = match path.split_once('.') {
        Some((h, r)) => (h, Some(r)),
        None => (path, None),
    };
    let (name, each) = match head.strip_suffix("[]") {
        Some(n) => (n, true),
        None => (head, false),
    };
    let Some(object) = v.as_object_mut() else {
        return Vec::new();
    };
    match (rest, each) {
        (None, false) => object.remove(name).into_iter().collect(),
        (None, true) => match object.get_mut(name) {
            Some(Value::Array(items)) => std::mem::take(items),
            _ => Vec::new(),
        },
        (Some(rest), false) => object
            .get_mut(name)
            .map(|inner| take(inner, rest))
            .unwrap_or_default(),
        (Some(rest), true) => match object.get_mut(name) {
            Some(Value::Array(items)) => items.iter_mut().flat_map(|i| take(i, rest)).collect(),
            _ => Vec::new(),
        },
    }
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

/// How parameters left in a frame are compared.
enum Local<'a> {
    /// Through the two lists' records carried into their frames.
    InFrames(&'a [Compared], &'a [Compared]),
    /// Not at all (no file-space value to compare them with).
    Skipped,
}

/// The families whose records differ between *expected* and *got*, matched by their faces
/// (records on equal faces paired in order). Parameters other than the local paths of either are
/// compared as they are; the local paths as *local* says.
fn differences(
    expected: &[Compared],
    got: &[Compared],
    local: Local,
    grid: &mut usize,
) -> Vec<Difference> {
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
            let mut paths: Vec<&String> = a.local.iter().chain(&b.local).collect();
            paths.sort();
            paths.dedup();
            let (mut pa, mut pb) = (a.parameters.clone(), b.parameters.clone());
            for p in &paths {
                take(&mut pa, p);
                take(&mut pb, p);
            }
            if !agree(&pa, &pb, grid) {
                notes.push(format!("{:?}: {}", a.faces, disagreement(&pa, &pb)));
                continue;
            }
            if paths.is_empty() {
                continue;
            }
            let (le, lg) = match local {
                Local::InFrames(le, lg) => (le, lg),
                Local::Skipped => continue,
            };
            let (mut la, mut lb) = (le[i].parameters.clone(), lg[j].parameters.clone());
            let fields =
                |v: &mut Value| -> Vec<Value> { paths.iter().flat_map(|p| take(v, p)).collect() };
            let (fa, fb) = (Value::Array(fields(&mut la)), Value::Array(fields(&mut lb)));
            if !agree(&fa, &fb, grid) {
                notes.push(format!("{:?}: local {}", a.faces, disagreement(&fa, &fb)));
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

/// The records of *recognition* in its frame (its file-space records carried back by it).
fn in_frame(
    recognition: &Recognition,
    frame: &PartFrame,
    doc: &RecognitionDocument,
) -> Vec<Compared> {
    let back = map_features(
        recognition.features.clone(),
        &Rigid::from_frame(frame).inverse(),
    );
    compared(doc, &back.features, &BTreeMap::new())
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
    let expected = compared(d0, &r0.features, &BTreeMap::new());
    let got = compared(d1, &back.features, &back.local);
    match (&r0.frame, &r1.frame) {
        (RecordFrame::Inferred(f0), RecordFrame::Inferred(f1)) => {
            // Where the gauge leaves the frame free (a sign, an interchange, a roll about the
            // axis), the moved part's frame can be another representative: its records are then
            // compared all the same, and fields the freedom moves differ.
            let (l0, l1) = (in_frame(r0, f0, d0), in_frame(r1, f1, d1));
            let mut found = differences(&expected, &got, Local::InFrames(&l0, &l1), grid);
            if !frames_agree(&moved_frame(f0, motion), f1) {
                for d in &mut found {
                    d.text = format!("(frames differ: {:?} gauge) {}", f0.gauge, d.text);
                }
            }
            found
        }
        (RecordFrame::Refused { reason: a }, RecordFrame::Refused { reason: b }) if a == b => {
            differences(&expected, &got, Local::Skipped, grid)
        }
        (a, b) => vec![Difference {
            family: "frame".into(),
            count: 1,
            text: format!("unmoved {a:?}, moved {b:?}"),
        }],
    }
}

/// `known_framed_document.json`'s entries, keyed by (file, check, family), with the most
/// records each may differ by.
fn known_entries() -> Vec<((String, String, String), usize)> {
    let known = common::load("known_framed_document.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_framed_document.json", known);
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
/// and every listed entry of them no longer seen.
fn unexplained(
    checks: &[&str],
    found: impl Iterator<Item = (String, String, Vec<Difference>)>,
) -> Vec<String> {
    let known: Vec<_> = known_entries()
        .into_iter()
        .filter(|((_, check, _), _)| checks.contains(&check.as_str()))
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
    for (key, _) in &known {
        if !seen.contains(key) {
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
/// `tests/fixtures/known_framed_document.json` under the motion's name.
#[test]
fn the_default_document_moves_with_the_part() {
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
    let files = corpus_files();
    let found = common::parallel::map(&files, |name| {
        let path = dir.join(name);
        // The corpus test reports read failures.
        let Ok((part, recognition, _)) = serve::recognise_step(&path.to_string_lossy(), &IDENTITY)
        else {
            return Vec::new();
        };
        let document = recognition::document(&recognition, part.faces.len());
        let unmoved = (recognition, document);
        motions
            .iter()
            .map(|(motion, placement)| {
                let mut grid = 0;
                let moved = default(&path, placement);
                let d = under_motion(
                    &unmoved,
                    &moved,
                    &Rigid::from_placement(placement),
                    &mut grid,
                );
                (motion.to_string(), d, grid)
            })
            .collect()
    });
    let mut grid = 0;
    let found: Vec<(String, String, Vec<Difference>)> = files
        .iter()
        .zip(found)
        .flat_map(|(name, cases)| {
            cases
                .into_iter()
                .map(move |(motion, d, g)| (name.clone(), motion, d, g))
        })
        .map(|(name, motion, d, g)| {
            grid += g;
            (name, motion, d)
        })
        .collect();
    eprintln!("values agreeing only to the two-decimal grid: {grid}");
    let checks: Vec<&str> = motions.iter().map(|m| m.0).collect();
    let problems = unexplained(&checks, found.into_iter());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The check name of [`the_default_document_agrees_with_caller_space_recognition`].
const CALLER_SPACE: &str = "caller_space";

/// On every corpus part, the default document against caller-space recognition
/// (`correspondence::recognise`, Python's entry points' space) of the part as read: the same
/// features on the same faces, every parameter in the file's coordinates agreeing as
/// [`the_default_document_moves_with_the_part`] compares them. A parameter the default leaves in
/// the frame (a part whose frame is not the file's axes up to order and sign) has no file-space
/// value to compare. Every difference is listed in `tests/fixtures/known_framed_document.json`
/// under `caller_space`, with a verdict.
#[test]
fn the_default_document_agrees_with_caller_space_recognition() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let files = corpus_files();
    let found = common::parallel::map(&files, |name| {
        let path = dir.join(name);
        let Ok((part, framed, _)) = serve::recognise_step(&path.to_string_lossy(), &IDENTITY)
        else {
            return (Vec::new(), 0, 0);
        };
        let caller = correspondence::recognise(&part);
        let (df, dc) = (
            recognition::document(&framed, part.faces.len()),
            recognition::document(&caller, part.faces.len()),
        );
        let local = df.records.iter().filter(|r| !r.local.is_empty()).count();
        let mut grid = 0;
        let d = differences(
            &compared(&dc, &caller.features, &BTreeMap::new()),
            &compared(&df, &framed.features, &BTreeMap::new()),
            Local::Skipped,
            &mut grid,
        );
        (d, grid, local)
    });
    let (mut grid, mut local) = (0, 0);
    let found: Vec<(String, String, Vec<Difference>)> = files
        .iter()
        .zip(found)
        .map(|(name, (d, g, l))| {
            grid += g;
            local += l;
            (name.clone(), CALLER_SPACE.to_string(), d)
        })
        .collect();
    eprintln!(
        "against caller space: {grid} values agree only to the two-decimal grid; {local} records \
         have fields left in a frame that is not the file's axes"
    );
    let problems = unexplained(&[CALLER_SPACE], found.into_iter());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
