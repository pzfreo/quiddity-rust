//! Volume probes against Python's booleans: every probe the Python recognisers ask over the
//! corpus (`tools/capture_probes.py`), answered by `kernel::volume`.

mod common;

use std::collections::BTreeMap;
use std::io::Read;

use quiddity::kernel::classify::Classifier;
use quiddity::kernel::geom::Bounds;
use quiddity::kernel::rays::RayCaster;
use quiddity::kernel::volume::{Probe, common_volume, probe_volume};
use quiddity::{Part, read_step, read_step_file};
use serde_json::Value;

fn probes() -> Vec<Value> {
    let path = common::fixtures().join("probes.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    let all: Value = serde_json::from_str(&text).unwrap();
    all["probes"].as_array().unwrap().clone()
}

#[test]
fn probes_match_python() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let mut by_file: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for p in probes() {
        by_file
            .entry(p["file"].as_str().unwrap().to_string())
            .or_default()
            .push(p);
    }
    let known: Vec<Value> = serde_json::from_value(common::load("known_probes.json")).unwrap();
    common::check_verdicts("known_probes.json", &known);
    let mut problems = Vec::new();
    for (file, asked) in by_file {
        let part = read_step_file(&dir.join(&file)).unwrap();
        let solids: Vec<Classifier<'_>> = (0..part.solids.len())
            .map(|s| Classifier::for_solid(&part, s))
            .collect();
        for p in asked {
            let (ok, report) = check(&p, &solids);
            if !ok {
                problems.push(format!("{file} {} {report}", p["caller"]));
            }
        }
    }
    let listed = |problem: &str, k: &Value| {
        problem.starts_with(&format!("{} ", k["file"].as_str().unwrap()))
            && problem.contains(k["contains"].as_str().unwrap())
    };
    let unexpected: Vec<&String> = problems
        .iter()
        .filter(|p| !known.iter().any(|k| listed(p, k)))
        .collect();
    let stale: Vec<&Value> = known
        .iter()
        .filter(|k| !problems.iter().any(|p| listed(p, k)))
        .collect();
    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "{} unexpected:\n{}\nstale: {stale:?}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Whether the port agrees with Python's answer (empty, full or the fraction), and a report.
fn check(p: &Value, solids: &[Classifier<'_>]) -> (bool, String) {
    let probe_part: Part;
    let (probe, shape) = if let Some(b) = p.get("box") {
        let c: Vec<f64> = serde_json::from_value(b.clone()).unwrap();
        let bounds = Bounds {
            min: [c[0], c[1], c[2]],
            max: [c[3], c[4], c[5]],
        };
        (Probe::Box(bounds), "box")
    } else {
        probe_part = read_step(p["step"].as_str().unwrap().as_bytes()).unwrap();
        (Probe::Solid(RayCaster::for_solid(&probe_part, 0)), "solid")
    };
    let got: f64 = p["solids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| common_volume(&solids[s.as_u64().unwrap() as usize], &probe))
        .sum();
    // Each side as a fraction of its own measure of the probe: the two kernels' probe volumes
    // agree only to about 1e-9.
    let (want, pv) = (
        p["volume"].as_f64().unwrap(),
        p["probe_volume"].as_f64().unwrap(),
    );
    let rpv = probe_volume(&probe);
    let kind = if want == 0.0 {
        "empty"
    } else if (want - pv).abs() <= 1e-9 * pv {
        "full"
    } else {
        "partial"
    };
    let ok = match kind {
        "empty" => got == 0.0,
        "full" => (got - rpv).abs() <= 1e-9 * rpv,
        _ => (got / rpv - want / pv).abs() <= 1e-6,
    };
    let report = format!("{shape}: python {want} rust {got} of {pv} (rust probe volume {rpv})");
    (ok, report)
}
