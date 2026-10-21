//! The snapshot manifest pins complete expected and input inventories, not just outputs that
//! happen to be reached by one developer's optional-corpus run.
use super::common;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
fn files(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files(&p, root, out);
        } else {
            out.insert(
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                hash(&p),
            );
        }
    }
}
fn hash(path: &Path) -> String {
    Sha256::digest(std::fs::read(path).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[test]
fn snapshot_provenance_pins_every_expected_file_and_input() {
    let fixture_root = common::fixtures();
    let root = fixture_root.join("rust-snapshot");
    let p: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(root.join("provenance.json")).unwrap())
            .unwrap();
    let source = p["source_commit"].as_str().unwrap();
    assert!(source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(!p["release_anchor"].as_str().unwrap().is_empty());
    let mut actual = BTreeMap::new();
    files(&root, &root, &mut actual);
    actual.remove("provenance.json");
    let expected: BTreeMap<String, String> =
        serde_json::from_value(p["snapshots"].clone()).unwrap();
    assert_eq!(
        actual, expected,
        "snapshot files changed without regenerating provenance"
    );
    let mut inputs = BTreeMap::new();
    files(&fixture_root, &fixture_root, &mut inputs);
    inputs.retain(|k, _| !k.starts_with("rust-snapshot/"));
    let mut actual: BTreeMap<String, String> = inputs
        .into_iter()
        .map(|(k, v)| (format!("tests/fixtures/{k}"), v))
        .collect();
    let corpus = common::corpus_dir();
    let mut expected: BTreeMap<String, String> =
        serde_json::from_value(p["inputs"].clone()).unwrap();
    if let Some(dir) = corpus {
        for row in common::load("corpus.json")["files"].as_array().unwrap() {
            let name = row["file"].as_str().unwrap();
            actual.insert(
                format!("corpus/{name}"),
                hash(&dir.join(PathBuf::from(name))),
            );
        }
    } else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        expected.retain(|k, _| !k.starts_with("corpus/"));
    }
    assert_eq!(
        actual, expected,
        "snapshot inputs changed; regenerate and review the snapshot"
    );
}
