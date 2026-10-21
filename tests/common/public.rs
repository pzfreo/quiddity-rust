#![allow(dead_code)]
#[path = "parallel.rs"]
pub mod parallel;
use std::path::{Path, PathBuf};
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
pub fn corpus_dir() -> Option<PathBuf> {
    std::env::var_os("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}
