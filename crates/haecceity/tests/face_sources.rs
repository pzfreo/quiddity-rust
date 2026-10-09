//! Parts and their face and edge numbering against specify-core's: for every file
//! `tools/capture_face_sources.py` captured (the shared corpus, NIST's AP242 PMI test models and
//! `tests/fixtures/ap242/assembly/assembly.step`), [`read_part_definitions`] must give the parts
//! specify-core's loader gives through OpenCascade XCAF (`specify_core.load.load_all`): as many,
//! in the same order, with the same names, the same placements (to 1e-9), and the same
//! `ADVANCED_FACE` / `EDGE_CURVE` instances (`#N`) in the same order, except for the differences
//! listed in `tests/fixtures/known_face_sources.json`.
//!
//! Every difference is a problem with an exact identity, a file and a key, and a list entry names
//! that identity with the number of problems it explains and a verdict, in the style of
//! `tests/corpus.rs`:
//!
//! ```json
//! {"file": "nist-pmi/nist_ctc_01_asme1_ap242-e1.stp", "key": "part 0 edges OpenCascade made",
//!  "count": 4, "verdict": "rust-correct", "reason": "..."}
//! ```
//!
//! Keys: `read` (one side refuses the file and the other reads it; one problem), `part count`
//! (one), and per part *i* of haecceity's, compared with the OpenCascade part that numbers the
//! most of its faces (else OpenCascade's part *i*): `part <i> name` (one), `part <i> placements`
//! (one per placement that differs or only one side has), and for each of `faces` and `edges`,
//! compared by instance, not by position:
//!
//! - `part <i> edges OpenCascade made`: one per OpenCascade index with no file instance (`null`,
//!   an entity its healing made);
//! - `part <i> edges OpenCascade only`: one per instance OpenCascade numbers in the part and
//!   haecceity does not;
//! - `part <i> edges in another OpenCascade part`: one per instance of haecceity's part that
//!   OpenCascade numbers in another of its parts;
//! - `part <i> edges haecceity only`: one per instance of haecceity's part OpenCascade numbers
//!   nowhere;
//! - `part <i> edges order`: one per instance both number that lies outside a longest common run
//!   of the two orders (the fewest that would have to move for them to agree), so an insertion on
//!   one side does not count against every later index.
//!
//! The test fails on a problem no entry names, on an entry whose count differs from the problems
//! it names (including an entry that names none), on two entries with one identity, and on a
//! checked file whose sha256 is not the one captured (it must be recaptured, not listed). A
//! failure prints each unlisted problem group as the entry it needs, with its first few details.
//!
//! Corpus files are checked when the corpus is found (`QUIDDITY_CORPUS`; required with
//! `QUIDDITY_CORPUS_REQUIRED=1`), NIST's when `HAECCEITY_NIST_PMI` names the directory of their
//! `NIST-PMI-STEP-Files` (required with `HAECCEITY_NIST_PMI_REQUIRED=1`); entries of files not
//! checked are not judged.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;

use haecceity::step::{PartDefinition, read_part_definitions};
use serde_json::Value;

const PLACEMENT: f64 = 1e-9;

/// The problem key of a file whose bytes are not those captured; no entry may list it.
const CHANGED: &str = "changed since captured";

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

/// The file's contents (decompressed when gzipped) and the sha256 of its bytes as stored.
fn bytes(path: &std::path::Path) -> (Vec<u8>, String) {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let digest = hex(&sha256(&raw));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut out)
            .unwrap();
        (out, digest)
    } else {
        (raw, digest)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 (FIPS 180-4), to check the digests without a dependency (as `tests/corpus_files.rs`,
/// whose test checks it against the standard vectors).
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// Problems by (file, key), each with its details.
type Problems = BTreeMap<(String, String), Vec<String>>;

/// For each element of *seq*, whether it lies on one longest increasing subsequence (patience
/// sorting; the first such subsequence found, so the result is deterministic).
fn on_longest_run(seq: &[usize]) -> Vec<bool> {
    let mut tails: Vec<usize> = Vec::new(); // index into seq of the smallest tail per length
    let mut before: Vec<Option<usize>> = vec![None; seq.len()];
    for (k, &x) in seq.iter().enumerate() {
        let len = tails.partition_point(|&t| seq[t] < x);
        before[k] = len.checked_sub(1).map(|l| tails[l]);
        if len == tails.len() {
            tails.push(k);
        } else {
            tails[len] = k;
        }
    }
    let mut on = vec![false; seq.len()];
    let mut k = tails.last().copied();
    while let Some(i) = k {
        on[i] = true;
        k = before[i];
    }
    on
}

/// One part's faces or edges against OpenCascade's, by instance, under keys `<what> <kind>`:
/// `OpenCascade made` (one per OpenCascade index with no file instance: an entity its healing
/// made), `OpenCascade only` (one per instance haecceity's part lacks), `in another OpenCascade
/// part` (one per instance of haecceity's part that OpenCascade numbers in another of its parts),
/// `haecceity only` (one per instance of haecceity's part OpenCascade numbers nowhere), and
/// `order` (one per instance both number that lies outside a longest run their two orders have
/// in common: the fewest that would have to move for the orders to agree).
fn compare_ids(
    what: &str,
    got: &[u64],
    want: &[Value],
    elsewhere: &BTreeMap<u64, usize>,
    add: &mut impl FnMut(String, String),
) {
    let mine: BTreeMap<u64, usize> = got.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut theirs: BTreeMap<u64, usize> = BTreeMap::new();
    for (i, w) in want.iter().enumerate() {
        match w.as_u64() {
            None => add(format!("{what} OpenCascade made"), format!("{i}: null")),
            Some(w) => {
                theirs.insert(w, i);
                if !mine.contains_key(&w) {
                    add(format!("{what} OpenCascade only"), format!("{i}: #{w}"));
                }
            }
        }
    }
    for (i, g) in got.iter().enumerate() {
        if theirs.contains_key(g) {
            continue;
        }
        match elsewhere.get(g) {
            Some(part) => add(
                format!("{what} in another OpenCascade part"),
                format!("{i}: #{g} (OpenCascade part {part})"),
            ),
            None => add(format!("{what} haecceity only"), format!("{i}: #{g}")),
        }
    }
    let shared: Vec<u64> = want
        .iter()
        .filter_map(Value::as_u64)
        .filter(|w| mine.contains_key(w))
        .collect();
    let positions: Vec<usize> = shared.iter().map(|w| mine[w]).collect();
    for (k, on) in on_longest_run(&positions).into_iter().enumerate() {
        if !on {
            let id = shared[k];
            add(
                format!("{what} order"),
                format!(
                    "#{id}: haecceity {}, OpenCascade {}",
                    mine[&id], theirs[&id]
                ),
            );
        }
    }
}

fn ids(part: &Value, kind: &str) -> Vec<u64> {
    part[kind]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_u64)
        .collect()
}

/// The OpenCascade part haecceity's part *i* is compared with: the one that numbers the most of
/// its faces (the first on a tie), or part *i* when none does.
fn counterpart(i: usize, got: &PartDefinition, want: &[Value]) -> Option<usize> {
    let mine: BTreeSet<u64> = got.faces.iter().copied().collect();
    let (best, shared) = want
        .iter()
        .enumerate()
        .map(|(j, w)| {
            (
                j,
                ids(w, "faces").iter().filter(|f| mine.contains(f)).count(),
            )
        })
        .fold((0, 0), |a, b| if b.1 > a.1 { b } else { a });
    if shared > 0 {
        Some(best)
    } else {
        (i < want.len()).then_some(i)
    }
}

fn compare_part(
    file: &str,
    i: usize,
    got: &PartDefinition,
    want: &[Value],
    j: usize,
    out: &mut Problems,
) {
    let mut add = |key: String, detail: String| {
        out.entry((file.to_string(), format!("part {i} {key}")))
            .or_default()
            .push(detail);
    };
    let other = &want[j];
    let name = other["name"].as_str().unwrap();
    if got.name != name {
        add(
            "name".into(),
            format!("{:?} vs OpenCascade part {j} {name:?}", got.name),
        );
    }
    for (kind, mine) in [("faces", &got.faces), ("edges", &got.edges)] {
        let elsewhere: BTreeMap<u64, usize> = want
            .iter()
            .enumerate()
            .filter(|&(k, _)| k != j)
            .flat_map(|(k, w)| ids(w, kind).into_iter().map(move |id| (id, k)))
            .collect();
        compare_ids(
            kind,
            mine,
            other[kind].as_array().unwrap(),
            &elsewhere,
            &mut add,
        );
    }
    let placements = other["placements"].as_array().unwrap();
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
            add(
                "placements".into(),
                format!("{p}: {g:?} vs OpenCascade {w:?}"),
            );
        }
    }
}

/// One captured file's problems.
fn check_file(entry: &Value, path: &std::path::Path) -> Problems {
    let file = entry["file"].as_str().unwrap();
    let mut out = Problems::new();
    let (bytes, digest) = bytes(path);
    if Some(digest.as_str()) != entry["sha256"].as_str() {
        out.insert(
            (file.to_string(), CHANGED.into()),
            vec![format!(
                "sha256 {digest}, captured from {}: recapture with tools/capture_face_sources.py",
                entry["sha256"]
            )],
        );
        return out;
    }
    let got = read_part_definitions(&bytes).map_err(|e| e.to_string());
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
    for (i, g) in got.iter().enumerate() {
        if let Some(j) = counterpart(i, g, want) {
            compare_part(file, i, g, want, j, &mut out);
        }
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
    for (file, key) in listed.keys() {
        if key == CHANGED {
            failures.push(format!("{file}: an entry lists {CHANGED:?}"));
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
        if key == CHANGED {
            failures.push(format!("{file}: {}", details[0]));
            continue;
        }
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
