//! The B-rep writer's round trip (`haecceity::brep_write`, design: `docs/brep-writer.md`): every
//! fixture part, and every corpus part when `QUIDDITY_CORPUS` names the corpus, is read, written
//! and read again. The part read back must be the part read first, bit for bit (faces, edges and
//! solids in the same order, the same geometry, so the same areas and volumes), recognise to the
//! same records and document, pass the EXPRESS schema and rule checks, and write to the same
//! bytes twice. A part the writer refuses must be refused by name. PMI round trips are checked
//! over the AP242 fixtures: what is carried reads back as written, and everything else is named.

#[path = "common/parallel.rs"]
mod parallel;

use std::io::Read;
use std::path::{Path, PathBuf};

use quiddity::kernel::brep_write::{WriteError, WriteOptions, write_step};
use quiddity::kernel::{self, pmi};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The shared corpus, as the corpus tests find it (`QUIDDITY_CORPUS`, `off` to skip).
fn corpus_dir() -> Option<PathBuf> {
    if std::env::var("QUIDDITY_CORPUS").is_ok_and(|v| v == "off") {
        return None;
    }
    let dir = std::env::var("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../quiddity/tests/corpus"));
    dir.is_dir().then_some(dir)
}

/// The STEP files under *dir*, sorted, skipping the subdirectories named.
fn step_files(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !skip.iter().any(|s| path.ends_with(s)) {
                    stack.push(path);
                }
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
            if [".step", ".stp", ".step.gz", ".stp.gz"]
                .iter()
                .any(|e| name.ends_with(e))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn file_bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

/// The recognition `quiddity <file.step>` prints (its `source_sha256` aside, which names the
/// file's bytes): the records and the document.
fn recognition(part: &kernel::Part) -> String {
    let recognition = quiddity::correspondence::recognise(part);
    let document = quiddity::recognition::document(&recognition, part.faces.len());
    let output = quiddity::serve::RecognitionOutput {
        recognition,
        document,
    };
    serde_json::to_string(&output).expect("records serialise")
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    /// Read back as the same part, recognised the same.
    Same,
    /// Refused by the writer, naming what it cannot write.
    Refused,
    /// The input is not a readable part (the broken fixtures).
    Unreadable,
}

/// Read, write and read *path*; the outcome, or every problem found.
fn round_trip(path: &Path) -> Result<Outcome, Vec<String>> {
    let name = path.display().to_string();
    let Ok(part) = kernel::read_step(&file_bytes(path)) else {
        return Ok(Outcome::Unreadable);
    };
    let options = WriteOptions {
        name: "round trip",
        pmi: None,
    };
    let (bytes, report) = match write_step(&part, &options) {
        Ok(written) => written,
        Err(WriteError::Unsupported(why)) if !why.is_empty() => return Ok(Outcome::Refused),
        Err(e) => return Err(vec![format!("{name}: {e}")]),
    };
    let mut problems = Vec::new();
    match write_step(&part, &options) {
        Ok((again, _)) if again == bytes => {}
        _ => problems.push(format!("{name}: a second write differs")),
    }
    if !report.face_order_preserved || !report.edge_order_preserved {
        problems.push(format!("{name}: face or edge order not preserved"));
    }
    if report.placements_flattened || !report.surface_kinds_changed.is_empty() {
        problems.push(format!("{name}: geometry not written as read: {report:?}"));
    }
    if !report.pcurves_not_written.is_empty() {
        problems.push(format!(
            "{name}: pcurves not written: {:?}",
            report.pcurves_not_written
        ));
    }
    let back = match kernel::read_step(&bytes) {
        Ok(back) => back,
        Err(e) => return Err(vec![format!("{name}: the written file does not read: {e}")]),
    };
    if (part.faces.len(), part.edges.len(), part.solids.len())
        != (back.faces.len(), back.edges.len(), back.solids.len())
    {
        problems.push(format!(
            "{name}: {} faces, {} edges, {} solids read back as {}, {}, {}",
            part.faces.len(),
            part.edges.len(),
            part.solids.len(),
            back.faces.len(),
            back.edges.len(),
            back.solids.len()
        ));
        return Err(problems);
    }
    for f in 0..part.faces.len() {
        if format!("{:?}", part.faces[f]) != format!("{:?}", back.faces[f]) {
            problems.push(format!("{name}: face {f} reads back otherwise"));
        }
        let (a, b) = (part.face_mass(f), back.face_mass(f));
        if a.map(|m| m[0].to_bits()) != b.map(|m| m[0].to_bits()) {
            problems.push(format!("{name}: face {f} area {a:?} reads back as {b:?}"));
        }
    }
    for e in 0..part.edges.len() {
        if format!("{:?}", part.edges[e]) != format!("{:?}", back.edges[e]) {
            problems.push(format!("{name}: edge {e} reads back otherwise"));
        }
    }
    for s in 0..part.solids.len() {
        if part.solids[s].faces != back.solids[s].faces {
            problems.push(format!("{name}: solid {s}'s faces read back otherwise"));
        }
        let (a, b) = (part.solid_mass(s), back.solid_mass(s));
        if a.map(|m| m.0.to_bits()) != b.map(|m| m.0.to_bits()) {
            problems.push(format!(
                "{name}: solid {s} volume {a:?} reads back as {b:?}"
            ));
        }
    }
    // A part read through a placement is written in its own frame and placed as the file placed
    // it: moving both alike must agree too (recognition moves parts into their own frame).
    let at = [
        [0.6, -0.8, 0.0, 1.5],
        [0.8, 0.6, 0.0, -2.0],
        [0.0, 0.0, 1.0, 3.0],
    ];
    let (moved, moved_back) = (
        part.transformed(&at).unwrap(),
        back.transformed(&at).unwrap(),
    );
    if (0..part.faces.len())
        .any(|f| format!("{:?}", moved.faces[f]) != format!("{:?}", moved_back.faces[f]))
        || (0..part.edges.len())
            .any(|e| format!("{:?}", moved.edges[e]) != format!("{:?}", moved_back.edges[e]))
    {
        problems.push(format!(
            "{name}: moved, the part read back differs from the part"
        ));
    }
    let doc = kernel::p21::Document::parse(bytes).unwrap();
    for v in kernel::express::validate_document(doc.graph())
        .iter()
        .take(3)
    {
        problems.push(format!("{name}: EXPRESS: {v}"));
    }
    for v in kernel::express_rules::check_all(&doc).iter().take(3) {
        problems.push(format!(
            "{name}: rule {} on #{} {}: {}",
            v.rule, v.id, v.entity, v.message
        ));
    }
    if problems.is_empty() && recognition(&part) != recognition(&back) {
        problems.push(format!("{name}: recognises otherwise"));
    }
    if problems.is_empty() {
        Ok(Outcome::Same)
    } else {
        Err(problems)
    }
}

/// Round-trip *files*: every problem, and the count of each outcome.
fn check(files: &[PathBuf]) -> std::collections::BTreeMap<String, usize> {
    let results = parallel::map(files, |p| round_trip(p));
    let mut counts = std::collections::BTreeMap::new();
    let mut problems = Vec::new();
    for r in results {
        match r {
            Ok(o) => *counts.entry(format!("{o:?}")).or_insert(0) += 1,
            Err(p) => {
                *counts.entry("Problem".to_owned()).or_insert(0) += 1;
                problems.extend(p);
            }
        }
    }
    eprintln!("{counts:?}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    counts
}

#[test]
fn fixture_parts_round_trip() {
    let counts = check(&step_files(&fixtures(), &["captured"]));
    // The two broken fixtures with geometry that did not resolve are refused, by name.
    assert_eq!(counts.get("Refused"), Some(&2), "{counts:?}");
    assert!(counts.get("Same").is_some_and(|&n| n > 200), "{counts:?}");
}

#[test]
fn captured_fixture_parts_round_trip() {
    let counts = check(&step_files(&fixtures().join("captured"), &[]));
    assert_eq!(counts.keys().collect::<Vec<_>>(), ["Same"], "{counts:?}");
}

#[test]
fn corpus_parts_round_trip() {
    let Some(dir) = corpus_dir() else {
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let counts = check(&step_files(&dir, &[]));
    assert_eq!(counts.keys().collect::<Vec<_>>(), ["Same"], "{counts:?}");
}

#[test]
fn unresolved_geometry_is_refused_by_name() {
    for (file, named) in [
        ("broken/unresolved_open_curve.step", "edge 1"),
        ("broken/unresolved_surface.step", "face 1"),
    ] {
        let part = kernel::read_step_file(&fixtures().join(file)).unwrap();
        let options = WriteOptions {
            name: "refused",
            pmi: None,
        };
        match write_step(&part, &options) {
            Err(WriteError::Unsupported(why)) => {
                assert!(why.iter().any(|w| w.starts_with(named)), "{file}: {why:?}")
            }
            other => panic!("{file}: not refused: {:?}", other.map(|(_, r)| r)),
        }
    }
}

/// Every AP242 fixture's parts with their PMI: what is carried reads back as written (in the
/// PMI writer's standard form), and the source's items not carried are exactly those named as
/// refused (with what refers to them).
#[test]
fn pmi_is_carried_or_refused_by_name() {
    let mut problems = Vec::new();
    let (mut carried_whole, mut with_refusals) = (0, 0);
    for path in step_files(&fixtures().join("ap242"), &[]) {
        let bytes = file_bytes(&path);
        let Ok(defs) = kernel::step::read_part_definitions(&bytes) else {
            continue;
        };
        let name = path.display().to_string();
        let doc = kernel::p21::Document::parse(bytes.clone()).unwrap();
        let read = pmi::read(&doc, &defs).unwrap();
        for (k, def) in defs.iter().enumerate() {
            let part = kernel::step::read_part(&bytes, def).unwrap();
            let source = &read.parts[k];
            let options = WriteOptions {
                name: &def.name,
                pmi: Some(source),
            };
            let (written, report) = match write_step(&part, &options) {
                Ok(w) => w,
                Err(e) => {
                    problems.push(format!("{name} part {k}: {e}"));
                    continue;
                }
            };
            let carried = report.pmi.expect("PMI was given");
            let refused: Vec<_> = carried.refused.iter().map(|r| r.item).collect();
            let mut expected = pmi::without(source, &refused);
            expected.holes.clear();
            if !pmi::write::differences_as_stated(&carried.written, &expected).is_empty() {
                problems.push(format!(
                    "{name} part {k}: what was written is not the source less what was named"
                ));
            }
            if carried.refused.iter().any(|r| r.why.is_empty()) {
                problems.push(format!("{name} part {k}: a refusal without a reason"));
            }
            let defs1 = kernel::step::read_part_definitions(&written).unwrap();
            let doc1 = kernel::p21::Document::parse(written).unwrap();
            let back = pmi::read(&doc1, &defs1).unwrap();
            for d in pmi::write::differences_as_stated(
                &pmi::write::standard_form(&carried.written),
                &back.parts[0],
            ) {
                problems.push(format!("{name} part {k}: {d}"));
            }
            if carried.refused.is_empty() {
                carried_whole += 1;
            } else {
                with_refusals += 1;
            }
        }
    }
    eprintln!("PMI carried whole: {carried_whole}; with refusals named: {with_refusals}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(carried_whole > 20, "{carried_whole}");
}
