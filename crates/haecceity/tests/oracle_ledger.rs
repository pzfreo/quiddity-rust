//! The oracle ledger (#52): per kernel query, how many cases agree with an independent
//! reference, how many differ with a settled verdict, and which are deferred and what each waits
//! on, with a ratchet against the committed `tests/oracle_ledger.json`.
//!
//! Everything is counted from evidence already in the repository: the captures of OpenCascade
//! (and NIST's and specify-core's expected PMI) that the comparison tests read, and the lists of
//! differences those tests pin with verdicts. Each comparison test fails on any unlisted
//! difference and on any listed case that agrees, so a case of a capture is agreed exactly when
//! no pinned difference names it; no reference kernel runs here. A difference is deferred when
//! its verdict is `undetermined` or `rust-wrong` (its reason says what it waits on); `rust-correct`,
//! `equivalent` and `not-applicable` settle it.
//!
//! The ratchet fails when a query's agreed count falls, or when a deferred case leaves the
//! ledger without being resolved (still listed with a settled verdict, or no longer listed while
//! its case is still captured, so it now agrees). Any other change to the counts fails as a stale
//! ledger: `HAECCEITY_LEDGER=write` rewrites it, and refuses to when the ratchet would fail.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use serde_json::{Value, json};

fn load(path: &Path) -> Value {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let text = if path.extension().is_some_and(|e| e == "gz") {
        let mut out = String::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_string(&mut out)
            .unwrap();
        out
    } else {
        String::from_utf8(raw).unwrap()
    };
    serde_json::from_str(&text).unwrap()
}

fn fixture(name: &str) -> Value {
    load(&common::fixtures().join(name))
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn text<'v>(v: &'v Value, k: &str) -> &'v str {
    v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}"))
}

/// One kernel query against one reference.
struct Query {
    name: &'static str,
    oracle: &'static str,
    test: &'static str,
    /// Every case the reference was captured for.
    cases: BTreeSet<String>,
    /// Pinned differences: case (or, for an item only the port has, its key) → verdict, reason.
    differences: BTreeMap<String, (String, String)>,
}

impl Query {
    fn new(name: &'static str, oracle: &'static str, test: &'static str) -> Self {
        Query {
            name,
            oracle,
            test,
            cases: BTreeSet::new(),
            differences: BTreeMap::new(),
        }
    }

    fn differ(&mut self, case: String, entry: &Value) {
        self.differences.insert(
            case,
            (
                text(entry, "verdict").to_owned(),
                text(entry, "reason").to_owned(),
            ),
        );
    }
}

const DEFERRED: [&str; 2] = ["undetermined", "rust-wrong"];

fn face_areas() -> Query {
    let mut q = Query::new(
        "face_area",
        "OpenCascade BRepGProp face areas (tests/fixtures/face_areas.json.gz); independent \
         integrations by tools/face_area_evidence.py and tools/kernel_evidence.py",
        "crates/haecceity/tests/face_areas.rs",
    );
    let known = fixture("references/face_areas.json");
    // A file the reader walks in another order than OpenCascade is one `order` case.
    let reordered: BTreeSet<&str> = arr(&known)
        .iter()
        .filter(|k| k["check"] == "order")
        .map(|k| text(k, "file"))
        .collect();
    for f in arr(&fixture("face_areas.json.gz")["files"]) {
        let file = text(f, "file");
        if reordered.contains(file) {
            q.cases.insert(format!("{file} order"));
        } else {
            for i in 0..arr(&f["area"]).len() {
                q.cases.insert(format!("{file} face {i}"));
            }
        }
    }
    for k in arr(&known) {
        let file = text(k, "file");
        match text(k, "check") {
            "occ" => q.differ(format!("{file} face {}", k["face"]), k),
            "order" => q.differ(format!("{file} order"), k),
            // The port against itself moved: not this reference.
            _ => {}
        }
    }
    q
}

fn face_moments() -> Query {
    let mut q = Query::new(
        "face_moments",
        "OpenCascade adaptive face areas and centroids (tests/fixtures/face_moments.json.gz); \
         Green's-theorem evidence in the test and tools/centroid_evidence.py",
        "crates/haecceity/tests/face_moments.rs",
    );
    for f in arr(&fixture("face_moments.json.gz")["files"]) {
        for i in 0..arr(&f["fine"]).len() {
            q.cases.insert(format!("{} face {i}", text(f, "file")));
        }
    }
    for k in arr(&fixture("references/face_moments.json")) {
        q.differ(format!("{} face {}", text(k, "file"), k["face"]), k);
    }
    q
}

fn classification() -> Query {
    let mut q = Query::new(
        "classification",
        "OpenCascade BRepClass3d_SolidClassifier states (tests/fixtures/classify.json.gz)",
        "crates/haecceity/tests/classify.rs",
    );
    for f in arr(&fixture("classify.json.gz")["files"]) {
        for i in 0..arr(&f["points"]).len() {
            q.cases.insert(format!("{} point {i}", text(f, "file")));
        }
    }
    for k in arr(&fixture("references/classify.json")) {
        // `parity` is the port's rays against each other, not against the reference.
        if k["check"] == "state" {
            q.differ(format!("{} point {}", text(k, "file"), k["point"]), k);
        }
    }
    q
}

fn rays() -> Query {
    let mut q = Query::new(
        "rays",
        "OpenCascade fuzzy-boolean probe volumes (tests/fixtures/probes.json.gz)",
        "crates/haecceity/tests/probes.rs",
    );
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for p in arr(&fixture("probes.json.gz")["probes"]) {
        let n = seen.entry(text(p, "file").to_owned()).or_default();
        q.cases.insert(format!("{} probe {n}", text(p, "file")));
        *n += 1;
    }
    for k in arr(&fixture("references/probe_evidence.json")) {
        for i in arr(&k["probes"]) {
            q.differ(format!("{} probe {i}", text(k, "file")), k);
        }
    }
    q
}

fn hlr() -> Query {
    let mut q = Query::new(
        "hlr",
        "OpenCascade hidden-line projections and sections (tests/fixtures/hlr.json.gz, \
         section.json.gz)",
        "crates/haecceity/tests/drawings.rs",
    );
    for p in arr(&fixture("hlr.json.gz")["parts"]) {
        for view in p["views"].as_object().into_iter().flat_map(|v| v.keys()) {
            q.cases.insert(format!("{} {view}", text(p, "file")));
        }
    }
    for p in arr(&fixture("section.json.gz")["parts"]) {
        for view in ["section", "cut"] {
            q.cases.insert(format!("{} {view}", text(p, "file")));
        }
    }
    for k in arr(&fixture("references/drawings.json")) {
        // `exact` is a floor on the port's own exact curves, not a comparison.
        if k["view"] != "exact" {
            q.differ(format!("{} {}", text(k, "file"), text(k, "view")), k);
        }
    }
    q
}

fn recovery() -> Query {
    let mut q = Query::new(
        "recovery",
        "OpenCascade face areas and perimeters behind recovered surfaces' nominal sizes \
         (tests/fixtures/captured/effective_surfaces/faces.json.gz)",
        "tests/internal/effective_surfaces.rs (recovery_lengths_match_opencascade_measurements)",
    );
    for r in arr(&fixture("captured/effective_surfaces/faces.json.gz")["runs"]) {
        for f in arr(&r["faces"]) {
            q.cases
                .insert(format!("{} face {}", text(r, "file"), f["face"]));
        }
    }
    for k in arr(&fixture("references/effective_surface_measurements.json")) {
        if k["field"] == "nominal" {
            for f in arr(&k["faces"]) {
                q.differ(format!("{} face {f}", text(k, "file")), k);
            }
        }
    }
    q
}

/// The PMI pins of one list, by key.
fn pmi_pins(list: &str) -> Vec<Value> {
    arr(&fixture("references/pmi.json")[list]).to_vec()
}

/// The first `words` words of `key`: `"m row 3 nominal"` names case `"m row 3"`.
fn leading(key: &str, words: usize) -> String {
    key.split(' ').take(words).collect::<Vec<_>>().join(" ")
}

fn pmi_nist() -> Query {
    let mut q = Query::new(
        "pmi_read_nist",
        "NIST's expected PMI rows (tests/fixtures/ap242/nist/*.expected.json)",
        "crates/haecceity/tests/pmi_read.rs (nist_expected_pmi)",
    );
    let dir = common::fixtures().join("ap242/nist");
    let mut models: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().into_owned();
            n.strip_suffix(".expected.json").map(str::to_owned)
        })
        .collect();
    models.sort();
    for m in &models {
        for row in arr(&load(&dir.join(format!("{m}.expected.json")))["rows"]) {
            q.cases.insert(format!("{m} row {}", row["row"]));
        }
    }
    for k in pmi_pins("nist") {
        let key = text(&k, "key");
        // `m row N …` names a row; `m extra …` an item NIST does not list.
        let case = if key.split(' ').nth(1) == Some("row") {
            leading(key, 3)
        } else {
            key.to_owned()
        };
        q.differ(case, &k);
    }
    q
}

fn pmi_occt() -> Query {
    let mut q = Query::new(
        "pmi_read_occt",
        "OpenCascade XCAF's semantic PMI (tests/fixtures/ap242/occt/*.json.gz), for the files \
         committed with their STEP",
        "crates/haecceity/tests/pmi_read.rs (opencascade_cross_check)",
    );
    let fx = common::fixtures();
    let mut names: Vec<String> = std::fs::read_dir(fx.join("ap242/occt"))
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().into_owned();
            n.strip_suffix(".json.gz").map(str::to_owned)
        })
        .collect();
    names.sort();
    let committed: Vec<String> = names
        .into_iter()
        .filter(|s| {
            fx.join(format!("ap242/nist/{s}.stp.gz")).exists()
                || fx.join(format!("ap242/specify/{s}.step.gz")).exists()
        })
        .collect();
    for stem in &committed {
        let occt = load(&fx.join(format!("ap242/occt/{stem}.json.gz")));
        for p in arr(&occt["parts"]) {
            for (kind, list) in [
                ("dimension", "dimensions"),
                ("tolerance", "geometric_tolerances"),
            ] {
                for o in arr(&p[list]) {
                    // Presentation only, or on no face: not compared.
                    if o["type"] != "DimensionPresentation" && !arr(&o["faces"]).is_empty() {
                        q.cases
                            .insert(format!("{stem} {kind} {}", text(o, "label")));
                    }
                }
            }
            for d in arr(&p["datums"]) {
                q.cases.insert(format!("{stem} datum {}", text(d, "name")));
            }
        }
    }
    for k in pmi_pins("occt") {
        let key = text(&k, "key");
        let words: Vec<&str> = key.split(' ').collect();
        // `stem occt dimension L` / `tolerance L` / `datum X …` name OpenCascade's items; the
        // others (`missing …`, materials, part definitions) items or files it has no case for.
        let case = match words.get(2) {
            Some(&("dimension" | "tolerance" | "datum")) if words.len() >= 4 => {
                format!("{} {} {}", words[0], words[2], words[3])
            }
            _ => key.to_owned(),
        };
        q.differ(case, &k);
    }
    q
}

fn pmi_specify() -> Query {
    let mut q = Query::new(
        "pmi_read_specify",
        "specify-core's intents for the PMI it wrote (tests/fixtures/ap242/specify/*.intent.json)",
        "crates/haecceity/tests/pmi_read.rs (specify_core_outputs)",
    );
    let dir = common::fixtures().join("ap242/specify");
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().into_owned();
            n.strip_suffix(".intent.json").map(str::to_owned)
        })
        .collect();
    cases.sort();
    for case in &cases {
        let intent = load(&dir.join(format!("{case}.intent.json")));
        let intents = match &intent {
            Value::Array(a) => a.clone(),
            one => vec![one.clone()],
        };
        for i in &intents {
            let part = &i["binding"]["part"];
            for (n, _) in arr(&i["requirements"]).iter().enumerate() {
                q.cases
                    .insert(format!("{case} part {part} requirement {n}"));
            }
        }
    }
    for k in pmi_pins("specify") {
        let key = text(&k, "key");
        // `case part P requirement N …` names a requirement; `case part P extra …` an item the
        // intent does not have.
        let case = if key.split(' ').nth(3) == Some("requirement") {
            leading(key, 5)
        } else {
            key.to_owned()
        };
        q.differ(case, &k);
    }
    q
}

fn queries() -> Vec<Query> {
    vec![
        face_areas(),
        face_moments(),
        classification(),
        rays(),
        hlr(),
        recovery(),
        pmi_nist(),
        pmi_occt(),
        pmi_specify(),
    ]
}

/// The ledger's record of a query.
fn record(q: &Query) -> Value {
    let agreed = q
        .cases
        .iter()
        .filter(|c| !q.differences.contains_key(*c))
        .count();
    let mut verdicts: BTreeMap<&str, usize> = BTreeMap::new();
    for (v, _) in q.differences.values() {
        *verdicts.entry(v.as_str()).or_default() += 1;
    }
    let deferred: Vec<Value> = q
        .differences
        .iter()
        .filter(|(_, (v, _))| DEFERRED.contains(&v.as_str()))
        .map(|(case, (verdict, reason))| {
            json!({
                "case": case,
                "captured": q.cases.contains(case),
                "verdict": verdict,
                "waits_on": reason,
            })
        })
        .collect();
    json!({
        "query": q.name,
        "oracle": q.oracle,
        "test": q.test,
        "cases": q.cases.len(),
        "agreed": agreed,
        "differences": verdicts,
        "deferred": deferred,
    })
}

/// Ratchet violations of `now` against the committed ledger `was`.
fn ratchet(was: &Value, now: &[Query]) -> Vec<String> {
    let mut out = Vec::new();
    for w in arr(&was["queries"]) {
        let name = text(w, "query");
        let Some(q) = now.iter().find(|q| q.name == name) else {
            out.push(format!("{name}: the query is gone"));
            continue;
        };
        let r = record(q);
        let (before, after) = (w["agreed"].as_u64(), r["agreed"].as_u64());
        if after < before {
            out.push(format!(
                "{name}: agreed fell from {} to {}",
                before.unwrap_or(0),
                after.unwrap_or(0)
            ));
        }
        for d in arr(&w["deferred"]) {
            let case = text(d, "case");
            let resolved = match q.differences.get(case) {
                // Still listed: deferred still, or settled by a verdict.
                Some(_) => true,
                // No longer listed: it agrees, if its case is still captured.
                None => q.cases.contains(case),
            };
            if !resolved {
                out.push(format!(
                    "{name}: deferred case {case:?} is gone without being resolved (its case is \
                     no longer captured)"
                ));
            }
        }
    }
    out
}

fn ledger_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_ledger.json")
}

fn ledger(now: &[Query]) -> Value {
    json!({
        "about": "Per kernel query: cases captured from an independent reference, agreed, \
                  differing by verdict, and deferred with what each waits on. Written by \
                  crates/haecceity/tests/oracle_ledger.rs (HAECCEITY_LEDGER=write); see there.",
        "queries": now.iter().map(record).collect::<Vec<_>>(),
    })
}

#[test]
fn oracle_ledger_holds_its_ratchet() {
    let now = queries();
    let path = ledger_path();
    let was = if path.exists() {
        load(&path)
    } else {
        json!({"queries": []})
    };
    let violations = ratchet(&was, &now);
    assert!(
        violations.is_empty(),
        "the oracle ledger's ratchet fails:\n{}",
        violations.join("\n")
    );
    let current = ledger(&now);
    for q in arr(&current["queries"]) {
        eprintln!(
            "{}: {} of {} agreed, differences {}, {} deferred",
            q["query"],
            q["agreed"],
            q["cases"],
            q["differences"],
            arr(&q["deferred"]).len()
        );
    }
    if std::env::var_os("HAECCEITY_LEDGER").is_some_and(|v| v == "write") {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&current).unwrap() + "\n",
        )
        .unwrap();
        return;
    }
    assert!(
        current == was,
        "tests/oracle_ledger.json is out of date: rerun with HAECCEITY_LEDGER=write and commit it"
    );
}

/// The ratchet itself: a fall in an agreed count, and a deferral gone unresolved, both fail.
#[test]
fn the_ratchet_refuses_a_drop_and_a_lost_deferral() {
    let now = queries();
    let current = ledger(&now);
    assert!(ratchet(&current, &now).is_empty());
    let mut higher = current.clone();
    let agreed = higher["queries"][0]["agreed"].as_u64().unwrap();
    higher["queries"][0]["agreed"] = json!(agreed + 1);
    assert_eq!(ratchet(&higher, &now).len(), 1);
    let mut lost = current;
    lost["queries"][0]["deferred"]
        .as_array_mut()
        .unwrap()
        .push(json!({"case": "no such file face 0", "captured": true}));
    assert_eq!(ratchet(&lost, &now).len(), 1);
}
