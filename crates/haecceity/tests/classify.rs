//! Point classification against OpenCascade: every point `tools/capture_classify.py` asked
//! `BRepClass3d_SolidClassifier` about over the corpus, answered by `classify::Classifier`, and
//! each point's probe rays checked for agreeing parity. Differences are listed, with verdicts
//! and evidence, in `tests/fixtures/known_classify.json`; an unlisted, stale or changed entry
//! fails.

mod common;

use std::collections::BTreeMap;
use std::io::Read;

use haecceity::classify::{Classifier, State};
use haecceity::read_step_file;
use haecceity::step::read_step_file_placed;
use serde_json::Value;

/// The 2026-10-08 review's H7 point: OpenCascade answers OUT; a B-spline thread flank (face 60)
/// whose crossing the seed grid missed made a rotated placement answer In.
#[test]
fn thread_flank_point_is_out_in_every_placement() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let path = dir.join("cadgenbench/threaded_connector_109.step");
    let p = [-54.258, -12.391, 397.522];
    let rotations = [
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [[0.6, -0.8, 0.0], [0.8, 0.6, 0.0], [0.0, 0.0, 1.0]],
        [[0.36, 0.48, -0.8], [-0.8, 0.6, 0.0], [0.48, 0.64, 0.6]],
    ];
    for r in rotations {
        let placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], 0.0]);
        let part = read_step_file_placed(&path, &placement).unwrap();
        let q = [0, 1, 2].map(|i| r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2]);
        let state = Classifier::new(&part).classify(q);
        assert!(
            matches!(state, State::Out | State::Unknown),
            "{state:?} under {r:?}"
        );
    }
}

fn captured() -> Value {
    let path = common::fixtures().join("classify.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn letter(state: State) -> char {
    match state {
        State::In => 'I',
        State::Out => 'O',
        State::On => 'N',
        State::Unknown => 'U',
    }
}

/// One difference: the file, the point's index, which check, and what was observed.
type Key = (String, u64, String);

#[test]
fn classification_matches_opencascade() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let mut found: BTreeMap<Key, String> = BTreeMap::new();
    let (mut points, mut disagreeing) = (0usize, 0usize);
    for f in captured()["files"].as_array().unwrap() {
        let file = f["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        let classifier = Classifier::new(&part);
        let states: Vec<char> = f["states"].as_str().unwrap().chars().collect();
        for (i, p) in f["points"].as_array().unwrap().iter().enumerate() {
            let p: [f64; 3] = serde_json::from_value(p.clone()).unwrap();
            points += 1;
            let got = letter(classifier.classify(p));
            if got != states[i] {
                let observed = format!("rust {got} occ {}", states[i]);
                found.insert((file.to_string(), i as u64, "state".into()), observed);
            }
            // Every clean ray must see the same parity, whatever the vote decides.
            let rays = classifier.explain(p);
            let parities: Vec<usize> = rays.iter().flatten().map(|n| n % 2).collect();
            if parities.windows(2).any(|w| w[0] != w[1]) {
                disagreeing += 1;
                let counts: Vec<String> = rays
                    .iter()
                    .map(|r| r.map_or("-".to_string(), |n| n.to_string()))
                    .collect();
                found.insert(
                    (file.to_string(), i as u64, "parity".into()),
                    counts.join(","),
                );
            }
        }
    }
    eprintln!("{points} points; clean rays disagree on parity at {disagreeing}");
    let known: Vec<Value> = serde_json::from_value(common::load("known_classify.json")).unwrap();
    common::check_verdicts("known_classify.json", &known);
    let mut listed: BTreeMap<Key, String> = BTreeMap::new();
    for k in &known {
        assert!(
            k["evidence"].is_string(),
            "known_classify.json: entry without evidence: {k}"
        );
        let key = (
            k["file"].as_str().unwrap().to_string(),
            k["point"].as_u64().unwrap(),
            k["check"].as_str().unwrap().to_string(),
        );
        let observed = k["observed"].as_str().unwrap().to_string();
        assert!(
            listed.insert(key, observed).is_none(),
            "known_classify.json: duplicate entry {k}"
        );
    }
    let show = |m: Vec<(&Key, &String)>| {
        m.iter()
            .map(|((f, i, c), o)| format!("  {f} #{i} {c}: {o}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let unexpected: Vec<_> = found
        .iter()
        .filter(|(k, o)| listed.get(*k) != Some(*o))
        .collect();
    let stale: Vec<_> = listed
        .iter()
        .filter(|(k, o)| found.get(*k) != Some(*o))
        .collect();
    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "{} unlisted or changed:\n{}\n{} stale or changed:\n{}",
        unexpected.len(),
        show(unexpected),
        stale.len(),
        show(stale)
    );
}
