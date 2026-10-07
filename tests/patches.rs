//! Support coverage against Python's face booleans: every `covered_patch` question the Python
//! recognisers ask over the corpus (`tools/capture_patches.py`), answered by `kernel::cover`.

mod common;

use std::io::Read;

use quiddity::kernel::cover::covered;
use quiddity::read_step;
use serde_json::Value;

#[test]
fn coverage_matches_python() {
    let path = common::fixtures().join("patches.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    let mut problems = Vec::new();
    let calls = all["patches"].as_array().unwrap();
    for c in calls {
        let read = |s: &Value| read_step(s.as_str().unwrap().as_bytes()).unwrap();
        let patch = read(&c["patch"]);
        let supports: Vec<_> = c["supports"].as_array().unwrap().iter().map(read).collect();
        let refs: Vec<_> = supports.iter().map(|p| (p, 0)).collect();
        let got = covered((&patch, 0), &refs);
        if got != c["covered"].as_bool().unwrap() {
            problems.push(format!(
                "{} {}: python {} rust {got}",
                c["file"], c["caller"], c["covered"]
            ));
        }
    }
    eprintln!("{} of {} agree", calls.len() - problems.len(), calls.len());
    assert!(
        problems.is_empty(),
        "{} differ:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
