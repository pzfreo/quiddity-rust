//! Rust support-coverage snapshots on every legacy patch/support STEP input.

mod common;

use std::io::Read;

use haecceity::cover::covered;
use haecceity::read_step;
use serde_json::Value;

#[test]
fn rust_snapshot_coverage() {
    let path = common::fixtures().join("patches.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let mut actual = Vec::new();
    let calls = all["patches"].as_array().unwrap();
    for c in calls {
        let read = |s: &Value| read_step(s.as_str().unwrap().as_bytes()).unwrap();
        let patch = read(&c["patch"]);
        let supports: Vec<_> = c["supports"].as_array().unwrap().iter().map(read).collect();
        let refs: Vec<_> = supports.iter().map(|p| (p, 0)).collect();
        let got = covered((&patch, 0), &refs);
        assert_eq!(
            Some(got),
            c["covered"].as_bool(),
            "OCCT patch {}: {}",
            actual.len(),
            c["caller"]
        );
        actual.push(got);
    }
    common::snapshot::check_with(
        &common::fixtures(),
        "coverage",
        &serde_json::json!(actual),
        &|a, b| a == b,
    );
}
