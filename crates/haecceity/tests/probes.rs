//! Rust volume-probe snapshots, independent ray geometry and OpenCascade fuzzy-boolean measurements.

mod common;

use std::collections::BTreeMap;
use std::io::Read;

use haecceity::classify::Classifier;
use haecceity::geom::Bounds;
use haecceity::rays::RayCaster;
use haecceity::volume::{Probe, common_volume, probe_volume};
use haecceity::{Part, read_step, read_step_file};
use serde_json::{Value, json};

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
fn rust_snapshot_probes() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let mut by_file: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for p in probes() {
        by_file
            .entry(p["file"].as_str().unwrap().into())
            .or_default()
            .push(p);
    }
    let by_file: Vec<_> = by_file.into_iter().collect();
    let actual = common::parallel::map(&by_file, |(file, asked)| {
        let part = read_step_file(&dir.join(file)).unwrap();
        let solids: Vec<Classifier<'_>> = (0..part.solids.len())
            .map(|s| Classifier::for_solid(&part, s))
            .collect();
        json!({"file":file,"probes":asked.iter().map(|p|measure(p,&solids)).collect::<Vec<_>>()})
    });
    let equal = |a: &Value, b: &Value| {
        let (a, b) = (a.as_array().unwrap(), b.as_array().unwrap());
        a.len() == b.len()
            && a.iter().zip(b).all(|(a, b)| {
                a["file"] == b["file"]
                    && a["probes"].as_array().unwrap().len()
                        == b["probes"].as_array().unwrap().len()
                    && a["probes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .zip(b["probes"].as_array().unwrap())
                        .all(|(a, b)| {
                            let (Some(g), Some(gv), Some(w), Some(wv)) = (
                                a["volume"].as_f64(),
                                a["probe_volume"].as_f64(),
                                b["volume"].as_f64(),
                                b["probe_volume"].as_f64(),
                            ) else {
                                return a == b;
                            };
                            if w == 0.0 {
                                g == 0.0
                            } else if (w - wv).abs() <= 1e-9 * wv {
                                (g - gv).abs() <= 1e-9 * gv
                            } else {
                                (g / gv - w / wv).abs() <= 1e-6
                            }
                        })
            })
    };
    common::snapshot::check_with(&common::fixtures(), "probes", &json!(actual), &equal);
}

/// nist_ftc_10: a line along y, just inside the wall of bore 200 (radius 1.75 along y), enters
/// the material at face 172 and the cross bore 175 (along x) at its near wall. It meets 175's
/// far wall 4.9e-4 inside bore 200, in the opening, but the file's B-spline edge between the
/// bores (edge 402, 4.4e-4 off 175 and 4.9e-4 off 200) is displaced into that opening, so the
/// trim of 175 claims the crossing; the cylinders themselves put it past bore 200, and the line
/// goes on in air.
#[test]
fn a_crossing_a_straying_edge_overshoots_yields_to_the_bore_across_it() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = read_step_file(&dir.join("nist/nist_ftc_10_asme1_rb.stp")).unwrap();
    let rays = RayCaster::for_solid(&part, 0);
    let origin = [42.79002251668521, -1.0, -25.36981851445912];
    let faces: Vec<usize> = rays
        .trimmed_hits(origin, [0.0, 1.0, 0.0], 100.0)
        .unwrap()
        .iter()
        .map(|h| h.face)
        .collect();
    assert_eq!(faces, [172, 175]);
}

/// Whether the port agrees with Python's answer (empty, full or the fraction), and a report.
fn measure(p: &Value, solids: &[Classifier<'_>]) -> Value {
    let probe_part: Part;
    let probe = if let Some(b) = p.get("box") {
        let c: Vec<f64> = serde_json::from_value(b.clone()).unwrap();
        Probe::Box(Bounds {
            min: [c[0], c[1], c[2]],
            max: [c[3], c[4], c[5]],
        })
    } else {
        probe_part = read_step(p["step"].as_str().unwrap().as_bytes()).unwrap();
        Probe::Solid(RayCaster::for_solid(&probe_part, 0))
    };
    let volume: Option<f64> = p["solids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| common_volume(&solids[s.as_u64().unwrap() as usize], &probe))
        .sum();
    json!({"volume":volume,"probe_volume":probe_volume(&probe)})
}

/// OpenCascade fuzzy booleans independently measured these two probes (1e-5 relative band).
#[test]
fn fuzzy_opencascade_probe_measurements() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let file = "nist/nist_ctc_02_asme1_rc.stp";
    let part = read_step_file(&dir.join(file)).unwrap();
    let solids: Vec<_> = (0..part.solids.len())
        .map(|s| Classifier::for_solid(&part, s))
        .collect();
    let inputs: Vec<_> = probes().into_iter().filter(|p| p["file"] == file).collect();
    for (index, expected) in [(4, 7.056070), (7, 7.799776)] {
        let measured = measure(&inputs[index], &solids)["volume"].as_f64().unwrap();
        assert!(
            (measured - expected).abs() < 1e-5 * expected,
            "probe {index}: {measured} vs {expected}"
        );
    }
}
