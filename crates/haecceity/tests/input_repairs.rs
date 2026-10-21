//! Audit all shared corpus and captured STEP inputs for unintended orientation repairs.
mod common;

use std::path::{Path, PathBuf};

fn captured_steps(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            captured_steps(&path, out);
        } else if path.to_str().is_some_and(|s| {
            [".step", ".stp", ".step.gz", ".stp.gz"]
                .iter()
                .any(|suffix| s.ends_with(suffix))
        }) {
            out.push(path);
        }
    }
}

#[test]
fn corpus_and_captured_inputs_have_no_inverted_shell_repairs() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "corpus required"
        );
        return;
    };
    let corpus: Vec<_> = common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| dir.join(entry["file"].as_str().unwrap()))
        .collect();
    let mut captured = Vec::new();
    captured_steps(&common::fixtures().join("captured"), &mut captured);
    captured.sort();
    for (name, paths) in [("corpus", corpus), ("captured", captured)] {
        assert!(!paths.is_empty(), "{name}: no inputs audited");
        let found = common::parallel::map(&paths, |path| {
            let part = haecceity::read_step_file(path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let nonpositive: Vec<_> = (0..part.solids.len())
                .filter_map(|s| {
                    let volume = part.solid_mass(s).map(|m| m.0);
                    (!volume.is_some_and(|v| v.is_finite() && v > 0.0)).then_some((s, volume))
                })
                .collect();
            (
                part.solids.len(),
                part.input_repairs().to_vec(),
                nonpositive,
            )
        });
        let solids: usize = found.iter().map(|r| r.0).sum();
        let repairs: Vec<_> = paths
            .iter()
            .zip(&found)
            .filter(|(_, r)| !r.1.is_empty())
            .map(|(p, r)| (p, &r.1))
            .collect();
        let nonpositive: Vec<_> = paths
            .iter()
            .zip(&found)
            .filter(|(_, r)| !r.2.is_empty())
            .map(|(p, r)| (p, &r.2))
            .collect();
        eprintln!(
            "{name}: {} files, {solids} solids, {} repaired inputs; nonpositive {nonpositive:?}",
            paths.len(),
            repairs.len()
        );
        assert!(repairs.is_empty(), "{name}: {repairs:?}");
        assert!(nonpositive.is_empty(), "{name}: {nonpositive:?}");
    }
}
