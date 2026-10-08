//! Parts and their face and edge numbering against specify-core's: for every file
//! `tools/capture_face_sources.py` captured (the shared corpus, NIST's AP242 PMI test models and
//! `tests/fixtures/ap242/assembly/assembly.step`), [`read_part_definitions`] must give the parts
//! specify-core's loader gives through OpenCascade XCAF (`specify_core.load.load_all`): as many,
//! in the same order, with the same names, the same placements (to 1e-9), and per face and edge
//! index the same `ADVANCED_FACE` / `EDGE_CURVE` instance (`#N`), except for the differences
//! listed in `tests/fixtures/known_face_sources.json`.
//!
//! Every difference is a problem with an exact identity, a file and a key, and a list entry names
//! that identity with the number of problems it explains and a verdict, in the style of
//! `tests/corpus.rs`:
//!
//! ```json
//! {"file": "nist-pmi/nist_ctc_01_asme1_ap242-e1.stp", "key": "part 0 edges",
//!  "count": 4, "verdict": "rust-correct", "reason": "..."}
//! ```
//!
//! Keys: `read` (one side refuses the file and the other reads it; one problem), `part count`
//! (one), and per part *i* present on both sides `part <i> name` (one), `part <i> faces` and
//! `part <i> edges` (one per index whose instance differs, plus one per index only one side has;
//! OpenCascade's `null`, an edge its healing made, differs from every instance) and
//! `part <i> placements` (one per placement that differs or only one side has). The test fails
//! on a problem no entry names, on an entry whose count differs from the problems it names
//! (including an entry that names none), and on two entries with one identity. A failure prints
//! each unlisted problem group as the entry it needs, with its first few details.
//!
//! Corpus files are checked when the corpus is found (`QUIDDITY_CORPUS`; required with
//! `QUIDDITY_CORPUS_REQUIRED=1`), NIST's when `HAECCEITY_NIST_PMI` names the directory of their
//! `NIST-PMI-STEP-Files` (required with `HAECCEITY_NIST_PMI_REQUIRED=1`); entries of files not
//! checked are not judged.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use haecceity::step::{PartDefinition, read_part_definitions};
use serde_json::Value;

const PLACEMENT: f64 = 1e-9;

fn captured() -> Value {
    let path = common::fixtures().join("face_sources.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// NIST's AP242 PMI test models, if `HAECCEITY_NIST_PMI` names their directory.
fn nist_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("HAECCEITY_NIST_PMI")?);
    dir.is_dir().then_some(dir)
}

fn required(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| v == "1")
}

/// Where a captured file is, if it is to be checked.
fn locate(file: &str) -> Option<PathBuf> {
    if let Some(name) = file.strip_prefix("corpus/") {
        let Some(dir) = common::corpus_dir() else {
            assert!(
                !required("QUIDDITY_CORPUS_REQUIRED"),
                "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
            );
            return None;
        };
        return Some(dir.join(name));
    }
    if let Some(name) = file.strip_prefix("nist-pmi/") {
        let Some(dir) = nist_dir() else {
            assert!(
                !required("HAECCEITY_NIST_PMI_REQUIRED"),
                "HAECCEITY_NIST_PMI_REQUIRED is set but HAECCEITY_NIST_PMI names no directory"
            );
            return None;
        };
        return Some(dir.join(name));
    }
    let name = file.strip_prefix("fixture/").expect("a known source");
    Some(common::fixtures().join(name))
}

fn bytes(path: &std::path::Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

/// Problems by (file, key), each with its details.
type Problems = BTreeMap<(String, String), Vec<String>>;

/// Instances by index on both sides: one problem per index that differs.
fn compare_ids(got: &[u64], want: &[Value], add: &mut impl FnMut(String)) {
    for i in 0..got.len().max(want.len()) {
        let (g, w) = (got.get(i), want.get(i).map(Value::as_u64));
        if g.copied() != w.flatten() || w.is_none() {
            add(format!(
                "{i}: {} vs OpenCascade {}",
                g.map_or("none".into(), |g| format!("#{g}")),
                match w {
                    None => "none".to_string(),
                    Some(None) => "null".to_string(),
                    Some(Some(w)) => format!("#{w}"),
                }
            ));
        }
    }
}

fn compare_part(file: &str, i: usize, got: &PartDefinition, want: &Value, out: &mut Problems) {
    let mut add = |key: &str, detail: String| {
        out.entry((file.to_string(), format!("part {i} {key}")))
            .or_default()
            .push(detail);
    };
    let name = want["name"].as_str().unwrap();
    if got.name != name {
        add("name", format!("{:?} vs OpenCascade {name:?}", got.name));
    }
    compare_ids(&got.faces, want["faces"].as_array().unwrap(), &mut |d| {
        add("faces", d)
    });
    compare_ids(&got.edges, want["edges"].as_array().unwrap(), &mut |d| {
        add("edges", d)
    });
    let placements = want["placements"].as_array().unwrap();
    for p in 0..got.placements.len().max(placements.len()) {
        let g = got.placements.get(p).map(|p| p.placement);
        let w: Option<Vec<Vec<f64>>> = placements.get(p).map(|w| {
            w.as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_f64().unwrap())
                        .collect()
                })
                .collect()
        });
        let same = match (&g, &w) {
            (Some(g), Some(w)) => {
                (0..3).all(|r| (0..4).all(|c| (g[r][c] - w[r][c]).abs() <= PLACEMENT))
            }
            _ => false,
        };
        if !same {
            add("placements", format!("{p}: {g:?} vs OpenCascade {w:?}"));
        }
    }
}

/// One captured file's problems.
fn check_file(entry: &Value, path: &std::path::Path) -> Problems {
    let file = entry["file"].as_str().unwrap();
    let mut out = Problems::new();
    let got = read_part_definitions(&bytes(path)).map_err(|e| e.to_string());
    let want = entry["parts"].as_array();
    let (got, want) = match (got, want, entry["error"].as_str()) {
        (Ok(got), Some(want), _) => (got, want),
        (Err(_), None, _) => return out,
        (got, _, error) => {
            let detail = match got {
                Ok(parts) => format!("{} parts read; OpenCascade: {error:?}", parts.len()),
                Err(e) => format!("refused ({e}); OpenCascade read it"),
            };
            out.insert((file.to_string(), "read".into()), vec![detail]);
            return out;
        }
    };
    if got.len() != want.len() {
        let names = |n: Vec<&str>| n.join(", ");
        out.insert(
            (file.to_string(), "part count".into()),
            vec![format!(
                "{} [{}] vs OpenCascade {} [{}]",
                got.len(),
                names(got.iter().map(|p| p.name.as_str()).collect()),
                want.len(),
                names(want.iter().map(|p| p["name"].as_str().unwrap()).collect()),
            )],
        );
    }
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        compare_part(file, i, g, w, &mut out);
    }
    out
}

#[test]
fn part_definitions_match_specify_core() {
    let captured = captured();
    let files: Vec<(&Value, PathBuf)> = captured["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| Some((e, locate(e["file"].as_str().unwrap())?)))
        .collect();
    let mut found = Problems::new();
    for problems in common::parallel::map(&files, |(entry, path)| check_file(entry, path)) {
        found.extend(problems);
    }
    let checked: Vec<&str> = files
        .iter()
        .map(|(e, _)| e["file"].as_str().unwrap())
        .collect();

    let known: Vec<Value> = common::load("known_face_sources.json")
        .as_array()
        .unwrap()
        .clone();
    common::check_verdicts("known_face_sources.json", &known);
    let mut listed: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut failures = Vec::new();
    for e in &known {
        let key = (
            e["file"].as_str().unwrap().to_string(),
            e["key"].as_str().unwrap().to_string(),
        );
        if listed
            .insert(key.clone(), e["count"].as_u64().unwrap())
            .is_some()
        {
            failures.push(format!("two entries for {key:?}"));
        }
    }
    for ((file, key), count) in &listed {
        if !checked.contains(&file.as_str()) {
            continue;
        }
        let n = found.get(&(file.clone(), key.clone())).map_or(0, Vec::len);
        if n as u64 != *count {
            failures.push(format!("{file} {key:?}: {count} listed, {n} found"));
        }
    }
    for ((file, key), details) in &found {
        if listed.contains_key(&(file.clone(), key.clone())) {
            continue;
        }
        let entry = serde_json::json!({
            "file": file, "key": key, "count": details.len(), "verdict": "", "reason": "",
        });
        let shown: Vec<&str> = details.iter().take(5).map(String::as_str).collect();
        failures.push(format!("{entry}\n    {}", shown.join("\n    ")));
    }
    assert!(
        failures.is_empty(),
        "{} problem(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
