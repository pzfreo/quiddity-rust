//! Volume probes against Python's booleans: every probe the Python recognisers ask over the
//! corpus (`tools/capture_probes.py`), answered by `kernel::volume`.

mod common;

use std::collections::BTreeMap;
use std::io::Read;

use haecceity::classify::Classifier;
use haecceity::geom::Bounds;
use haecceity::rays::RayCaster;
use haecceity::volume::{Probe, common_volume, probe_volume};
use haecceity::{Part, read_step, read_step_file};
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
    // A problem is keyed exactly: its file, its caller, and its index among that file's probes
    // in `probes.json.gz` order. Files are checked in parallel, the problems kept in file order.
    let by_file: Vec<(String, Vec<Value>)> = by_file.into_iter().collect();
    let problems: Vec<Problem> = common::parallel::map(&by_file, |(file, asked)| {
        let part = read_step_file(&dir.join(file)).unwrap();
        let solids: Vec<Classifier<'_>> = (0..part.solids.len())
            .map(|s| Classifier::for_solid(&part, s))
            .collect();
        let mut problems = Vec::new();
        for (index, p) in asked.iter().enumerate() {
            let (ok, report) = check(p, &solids);
            if !ok {
                let caller = p["caller"].as_str().unwrap().to_string();
                problems.push((file.clone(), caller, index, report));
            }
        }
        problems
    })
    .into_iter()
    .flatten()
    .collect();
    // An entry names the probes it covers; each must still be a problem, and each problem must
    // be covered by exactly one entry.
    let listed = |(file, caller, index, _): &Problem, k: &Value| {
        k["file"].as_str() == Some(file.as_str())
            && k["caller"].as_str() == Some(caller.as_str())
            && k["probes"]
                .as_array()
                .is_some_and(|ps| ps.iter().any(|i| i.as_u64() == Some(*index as u64)))
    };
    let mut failures = Vec::new();
    for problem in &problems {
        let n = known.iter().filter(|k| listed(problem, k)).count();
        if n != 1 {
            let (file, caller, index, report) = problem;
            failures.push(format!(
                "{n} entries for {file} {caller} #{index}: {report}"
            ));
        }
    }
    for k in &known {
        let want = k["probes"].as_array().map_or(0, |ps| ps.len());
        let got = problems.iter().filter(|p| listed(p, k)).count();
        if want == 0 || got != want {
            failures.push(format!("entry matches {got} of its {want} probes: {k}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A probe the port answers differently: file, caller, index in the file, report.
type Problem = (String, String, usize, String);

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
    // Each side as a fraction of its own measure of the probe: the two kernels' probe volumes
    // agree only to about 1e-9.
    let (want, pv) = (
        p["volume"].as_f64().unwrap(),
        p["probe_volume"].as_f64().unwrap(),
    );
    // A refusal is always a difference: Python answered every probe here.
    let got: Option<f64> = p["solids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| common_volume(&solids[s.as_u64().unwrap() as usize], &probe))
        .sum();
    let (Some(got), Some(rpv)) = (got, probe_volume(&probe)) else {
        return (
            false,
            format!("{shape}: python {want} of {pv}, rust refused"),
        );
    };
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
