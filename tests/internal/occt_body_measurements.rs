//! Solid bounds, volume and area embedded in the old body-key records were measured by OCCT.
//! Keep the measurements independently of Python's recognition choices and record structure.
use super::common;
use serde_json::{Value, json};
use std::io::Read;

#[test]
fn solid_measurements_match_opencascade() {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        std::fs::File::open(common::fixtures().join("references/body_measurements.json.gz"))
            .unwrap(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    let corpus = common::corpus_dir();
    assert!(corpus.is_some() || std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
    let problems = common::parallel::map(&rows, |row| {
        let file = row["file"].as_str().unwrap();
        let root = match row["source"].as_str().unwrap() {
            "corpus" => {
                let Some(dir) = corpus.as_ref() else {
                    return Vec::new();
                };
                dir.clone()
            }
            "captured" => common::fixtures().join("captured"),
            "fixture" => common::fixtures(),
            _ => unreachable!(),
        };
        let part = quiddity::read_step_file(&root.join(file)).unwrap();
        let mut actual: Vec<Value> = (0..part.solids.len())
            .filter_map(|s| {
                let b = part.solid_bounds(s);
                let (volume, area) = part.solid_mass(s)?;
                Some(json!([
                    b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2], volume, area
                ]))
            })
            .collect();
        // Some legacy callers measured the whole compound as one shape.
        if actual.len() > 1 {
            let b = part.bounds();
            let volume: f64 = actual.iter().map(|v| v[6].as_f64().unwrap()).sum();
            let area: f64 = actual.iter().map(|v| v[7].as_f64().unwrap()).sum();
            actual.push(json!([
                b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2], volume, area
            ]));
        }
        row["measurements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|expected| {
                !actual.iter().any(|got| {
                    let n = expected.as_array().unwrap().len();
                    common::same(
                        &Value::Array(got.as_array().unwrap()[..n].to_vec()),
                        expected,
                    )
                })
            })
            .map(|expected| {
                (
                    json!({"source":row["source"],"file":file,"measurement":expected}),
                    format!("{} {file}: OCCT {expected}, Rust {actual:?}", row["source"]),
                )
            })
            .collect::<Vec<_>>()
    })
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let known = common::load("references/body_measurement_evidence.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("references/body_measurement_evidence.json", known);
    let matches = |id: &Value, k: &Value| {
        ["source", "file", "measurement"]
            .iter()
            .all(|f| id[*f] == k[*f])
    };
    for (id, report) in &problems {
        assert_eq!(
            known.iter().filter(|k| matches(id, k)).count(),
            1,
            "unlisted or duplicate OCCT difference: {report}"
        );
    }
    for k in known
        .iter()
        .filter(|k| corpus.is_some() || k["source"] != "corpus")
    {
        assert_eq!(
            problems.iter().filter(|(id, _)| matches(id, k)).count(),
            1,
            "stale OCCT verdict: {k}"
        );
    }
}
