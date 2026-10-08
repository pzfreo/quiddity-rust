//! Shared by the kernel tests: the fixtures (kept with the workspace's), the shared STEP corpus,
//! and the lists of known differences from Python.

#![allow(dead_code)]

pub mod drawing;
#[path = "../../../../tests/common/parallel.rs"]
pub mod parallel;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The workspace's fixtures directory.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

pub fn load(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join(name)).unwrap()).unwrap()
}

/// The shared corpus: `QUIDDITY_CORPUS`, else the sibling Python checkout's.
pub fn corpus_dir() -> Option<PathBuf> {
    let dir = std::env::var("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../quiddity/tests/corpus")
        });
    dir.is_dir().then_some(dir)
}

/// Every listed difference must carry a verdict and a reason.
pub fn check_verdicts(list: &str, entries: &[Value]) {
    const VERDICTS: [&str; 5] = [
        "rust-correct",
        "rust-wrong",
        "equivalent",
        "undetermined",
        "not-applicable",
    ];
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
