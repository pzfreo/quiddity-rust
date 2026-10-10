//! Output does not depend on hash order: every corpus part (`corpus.json`'s files, as the corpus
//! tests read them) is read, recognised and corresponded with itself twice in one process, where
//! each `HashMap` gets a fresh random hash seed (the STEP reader's entity indices, the
//! recognisers' maps, the correspondence's face index), and the JSON the CLI would print — the
//! recognition with fingerprints, and `correspond` of the part with itself — must be the same
//! byte for byte. Two parts are also held to the maps they were chosen for: circular face
//! patterns on cgb203, thin walls on cgb241. A part whose runs differ is reported with the
//! families (or correspondence fields) that differ; a part that fails the same way twice is
//! reported with its failure and must be pinned in `FAILING_IN_BOTH_RUNS`.

mod common;
#[macro_use]
#[path = "support/slices.rs"]
mod slices;

use quiddity::correspondence;
use serde_json::Value;

/// Parts that must still exercise a family whose recogniser goes through a `HashMap`.
const GUARDED: [(&str, &str); 2] = [
    ("cadgenbench_inputs/cgb203.step", "circular_face_patterns"),
    ("cadgenbench_inputs/cgb241.step", "thin_wall_bodies"),
];

/// Parts whose read or correspondence with itself fails in both runs with the same message, and
/// which step failed (`read` or `correspond`): none. Such a part has nothing to compare, so it is
/// listed here rather than passing as deterministic (its message is printed), and a part that
/// starts or stops failing changes the list and fails the test. A read failure is also a `read`
/// problem in `tests/corpus.rs`, so the parts failing to read must be the `read` entries of
/// `known_divergences.json`. (`recognise` returns no error: a refusal it cannot carry panics,
/// which fails this test outright.)
const FAILING_IN_BOTH_RUNS: [(&str, &str); 0] = [];

/// What `quiddity <file.step>` prints, then what `quiddity correspond <file> <file>` prints, or
/// the read or correspondence error.
fn outputs(path: &std::path::Path) -> Result<(String, String), String> {
    // The default recognition, in the part's own frame, with its document (review M8).
    let (output, _) = quiddity::serve::recognise_file(&path.to_string_lossy())
        .map_err(|e| format!("read: {}", e.message))?;
    let fingerprints = &output.recognition.fingerprints;
    let c = correspondence::correspond(fingerprints, fingerprints)
        .map_err(|e| format!("correspond: {e}"))?;
    Ok((
        serde_json::to_string_pretty(&output).unwrap(),
        serde_json::to_string_pretty(&c).unwrap(),
    ))
}

/// The top-level keys whose values differ between two JSON documents.
fn differing_keys(a: &str, b: &str) -> Vec<String> {
    let (a, b): (Value, Value) = (
        serde_json::from_str(a).unwrap(),
        serde_json::from_str(b).unwrap(),
    );
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return vec!["(not an object)".to_owned()];
    };
    let mut keys: Vec<String> = a
        .keys()
        .chain(b.keys())
        .filter(|k| a.get(*k) != b.get(*k))
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

fn corpus_files() -> Vec<String> {
    common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_owned())
        .collect()
}

/// Every corpus part of slice *k* of *n* (`tests/support/slices.rs`) twice; a guarded part, and
/// an entry of `FAILING_IN_BOTH_RUNS`, is checked by the slice that has its file.
fn same_twice(k: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let all = corpus_files();
    let files = slices::slice(&all, k, n);
    // Each part twice, as two work items: the runs get fresh seeds and usually separate threads,
    // and the slowest part's two runs overlap rather than ending two passes.
    let runs: Vec<&String> = files.iter().flat_map(|name| [name, name]).collect();
    let found = common::parallel::map(&runs, |name| outputs(&dir.join(name)));
    let mut problems = Vec::new();
    let mut failing = Vec::new();
    for (name, pair) in files.iter().zip(found.chunks(2)) {
        match (&pair[0], &pair[1]) {
            (Ok(first), Ok(second)) => {
                if first.0 != second.0 {
                    problems.push(format!(
                        "{name}: recognition JSON differs between runs in {:?}",
                        differing_keys(&first.0, &second.0)
                    ));
                }
                if first.1 != second.1 {
                    problems.push(format!(
                        "{name}: correspondence JSON differs between runs in {:?}",
                        differing_keys(&first.1, &second.1)
                    ));
                }
            }
            (Err(a), Err(b)) if a == b => {
                eprintln!("{name}: fails in both runs: {a}");
                failing.push((name.as_str(), a.split(':').next().unwrap()));
            }
            (a, b) => problems.push(format!(
                "{name}: reading differs between runs: {:?} vs {:?}",
                a.as_ref().err(),
                b.as_ref().err()
            )),
        }
    }
    for (guarded, family) in GUARDED {
        assert!(all.iter().any(|f| f == guarded), "{guarded} is not in corpus.json");
        let Some(i) = files.iter().position(|f| f == guarded) else {
            continue;
        };
        let exercised = found[2 * i].as_ref().is_ok_and(|(recognition, _)| {
            let json: Value = serde_json::from_str(recognition).unwrap();
            json[family].as_array().is_some_and(|a| !a.is_empty())
        });
        assert!(exercised, "{guarded} no longer finds {family}");
    }
    eprintln!(
        "{} corpus parts recognised twice, {} failing in both runs",
        files.len(),
        failing.len()
    );
    let pinned: Vec<(&str, &str)> = FAILING_IN_BOTH_RUNS
        .into_iter()
        .filter(|(file, _)| slices::owns(&all, k, n, file))
        .collect();
    if failing != pinned {
        problems.push(format!(
            "parts failing in both runs {failing:?}, FAILING_IN_BOTH_RUNS (this slice's) {pinned:?}"
        ));
    }
    let known = common::load("known_divergences.json");
    let mut read_divergences: Vec<&str> = known
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["family"] == "read")
        .map(|e| e["file"].as_str().unwrap())
        .collect();
    read_divergences.sort();
    let mut unread: Vec<&str> = FAILING_IN_BOTH_RUNS
        .iter()
        .filter(|(_, step)| *step == "read")
        .map(|(file, _)| *file)
        .collect();
    unread.sort();
    assert_eq!(
        unread, read_divergences,
        "the parts FAILING_IN_BOTH_RUNS to read and known_divergences.json's read entries disagree"
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    recognition_json_is_the_same_twice_in_one_process,
    super::same_twice,
    [0 => slice_0, 1 => slice_1, 2 => slice_2]
);
