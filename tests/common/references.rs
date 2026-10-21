//! Input fixtures and independent reference helpers shared by public and private tests.
#![allow(dead_code)]
#[path = "parallel.rs"]
pub mod parallel;
#[path = "snapshot.rs"]
pub mod snapshot;

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Structural equality with -0.0 == 0.0, measured values agreeing to
/// one part in a million: below that, the two kernels' parameter-range arithmetic differs in its
/// last digits (sub-micron at part scale). Rounded fields still compare exactly at their grid.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            // Zero's sign is not compared: a record coordinate of ±0.0 is ~1e-17 of kernel
            // round-off rounded away, and OpenCascade's sign of it is not reproducible. Where a
            // sign could decide something (atan2 in patterns), the port uses Python's exact
            // arithmetic, and the pattern replay agrees even sign-strictly.
            (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn load(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join(name)).unwrap()).unwrap()
}

/// The shared corpus: `QUIDDITY_CORPUS`, else the sibling Python checkout's; none where
/// `QUIDDITY_CORPUS` is `off` (a quick local run: the corpus tests skip, as when it is missing).
pub fn corpus_dir() -> Option<PathBuf> {
    if std::env::var("QUIDDITY_CORPUS").is_ok_and(|v| v == "off") {
        return None;
    }
    let dir = std::env::var("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../quiddity/tests/corpus"));
    dir.is_dir().then_some(dir)
}

/// What a listed difference says about the port: `rust-correct` (the port has the true geometry;
/// the reason gives the evidence), `rust-wrong` (a port limitation), `equivalent` (one geometry,
/// two representations), `undetermined` (neither shown right) or `not-applicable` (the case
/// does not test the part).
pub const VERDICTS: [&str; 5] = [
    "rust-correct",
    "rust-wrong",
    "equivalent",
    "undetermined",
    "not-applicable",
];

/// Every listed difference must carry a verdict and a reason.
pub fn check_verdicts(list: &str, entries: &[Value]) {
    for e in entries {
        assert!(
            e["verdict"].as_str().is_some_and(|v| VERDICTS.contains(&v)),
            "{list}: entry without a verdict ({VERDICTS:?}): {e}"
        );
        assert!(
            e["reason"].is_string(),
            "{list}: entry without a reason: {e}"
        );
    }
}
