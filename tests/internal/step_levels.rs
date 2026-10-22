//! Rust snapshots of face levels, risers, proofs, aggregate candidates, inventories and accepted records, with the legacy retry settings as input.

use super::common;

use std::io::Read;
use std::path::Path;

use quiddity::Part;
use quiddity::features::evidence::{common_valid_solid, locally_valid_solid};
use quiddity::features::levels::{self, FaceLevel, RiserEvidence};
use quiddity::features::{self, Context, Occurrence};
use quiddity::kernel::step::read_step_file;
use serde_json::{Value, json};

const CAPTURE: &str = "captured/step_levels/capture.json.gz";

fn capture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(common::fixtures().join(CAPTURE)).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn parts(capture: &Value) -> &Vec<Value> {
    capture["parts"].as_array().unwrap()
}

/// The capture is of this corpus, at the revision `corpus.json` records (every file and its
/// sha256).
#[test]
fn capture_covers_the_corpus() {
    let capture = capture();
    let corpus = common::load("corpus.json");
    assert_eq!(capture["quiddity_revision"], corpus["quiddity_revision"]);
    let want: Vec<(&Value, &Value)> = corpus["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (&f["file"], &f["sha256"]))
        .collect();
    let got: Vec<(&Value, &Value)> = parts(&capture)
        .iter()
        .map(|p| (&p["file"], &p["sha256"]))
        .collect();
    assert_eq!(got, want);
}

fn read(dir: &Path, file: &str) -> Part {
    read_step_file(&dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn json<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}

/// Occurrences as Python's capture writes them: each record with its defining faces, sorted.
fn entries<R: serde::Serialize>(found: &[Occurrence<R>]) -> Value {
    Value::Array(
        found
            .iter()
            .map(|o| {
                let mut defining = o.defining.clone();
                defining.sort_unstable();
                json!({"record": json(&o.record), "defining": defining})
            })
            .collect(),
    )
}

/// Entries sorted by their defining faces (Python's riser proposals are in solid order).
fn by_faces(v: Value) -> Value {
    let mut items = v.as_array().unwrap().clone();
    items.sort_by_key(|e| e["defining"].to_string());
    Value::Array(items)
}

/// Each occurrence's proofs, strictly and under local degradation.
fn proofs<R>(part: &Part, found: &[Occurrence<R>]) -> Value {
    Value::Array(
        found
            .iter()
            .map(|o| {
                let mut defining = o.defining.clone();
                defining.sort_unstable();
                json!({
                    "defining": defining,
                    "strict": common_valid_solid(part, &o.defining).is_some(),
                    "degraded": locally_valid_solid(part, &o.defining).is_some(),
                })
            })
            .collect(),
    )
}

/// The aggregate's proved step levels and risers.
type Proved = (Vec<Occurrence<FaceLevel>>, Vec<Occurrence<RiserEvidence>>);

/// The checks one part fails: `(check, rust, python)`.
fn snapshot_part(dir: &Path, p: &Value) -> Value {
    let file = p["file"].as_str().unwrap();
    let part = read(dir, file);
    let ctx = Context::new(&part);
    let found = levels::discover_step_levels(&ctx);
    let risers = levels::discover_risers(&ctx, &[]);
    let degraded = p["aggregate"]["retried"].as_bool().unwrap();
    let proved: Result<Proved, String> = levels::proved(&part, found.clone(), degraded)
        .and_then(|levels_found| {
            let risers = levels::discover_risers(&ctx, &levels_found);
            Ok((levels_found, levels::proved(&part, risers, degraded)?))
        })
        .map_err(|e| e.to_string());
    let aggregate = proved
        .map(|(levels, risers)| json!({"step_levels":entries(&levels),"risers":entries(&risers)}));
    let inventory = features::inventory(&part).unwrap_or_else(|e| panic!("{file}: {e}"));
    let carried = |f: &features::Features, family: &str| -> Value {
        let records = json(f)[family].clone();
        json!(
            records
                .as_array()
                .unwrap()
                .iter()
                .zip(&f.defining[family])
                .map(|(r, d)| {
                    let mut defining = d.clone();
                    defining.sort_unstable();
                    json!({"record":r,"defining":defining})
                })
                .collect::<Vec<_>>()
        )
    };
    json!({"file":file,"records":levels::step_level_records(&part,None),"proposals":entries(&found),"proofs":proofs(&part,&found),
        "riser_proposals":by_faces(entries(&risers)),"riser_proofs":by_faces(proofs(&part,&risers)),"aggregate":aggregate,
        "inventory_step_levels":carried(&inventory.physical,"step_levels"),"inventory_risers":carried(&inventory.physical,"risers"),
        "accepted_risers":carried(&inventory.accepted(),"risers")})
}

#[test]
fn rust_snapshot_step_levels_and_risers() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let inputs = capture();
    let actual = common::parallel::map(parts(&inputs), |p| snapshot_part(&dir, p));
    common::snapshot::check(&common::fixtures(), "step_levels", &json!(actual));
}
