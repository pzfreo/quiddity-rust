//! Fillet evidence parity: hand-built cases exported by `tools/export_fixtures.py` must give the
//! same records, the same defining faces and the same evidence refusals as Python, with faces
//! walked in OpenCascade's order.

mod common;

use std::path::{Path, PathBuf};

use quiddity::features::Context;
use quiddity::features::fillets::{FilletOptions, discover_verified, recognise_fillets};
use quiddity::kernel::step::{IDENTITY, read_step_file_placed};
use quiddity::{Part, read_step_file};
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn manifest() -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join("manifest.json")).unwrap())
        .unwrap()
}

fn check_runs(name: &str, part: &Part, runs: &[Value], problems: &mut Vec<String>) {
    for run in runs {
        let opts: FilletOptions = common::options(&run["options"]);
        let got: Vec<Value> = recognise_fillets(part, &opts)
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect();
        let want = run["records"].as_array().unwrap();
        let same = got.len() == want.len()
            && got.iter().zip(want).all(|(g, w)| {
                g["axis"] == w["axis"]
                    && g["turned"] == w["turned"]
                    && g["side"] == w["side"]
                    && g["radius"].as_f64() == w["radius"].as_f64()
                    && (0..3).all(|k| g["at"][k].as_f64() == w["at"][k].as_f64())
            });
        if !same {
            let brief = |r: &Value| {
                format!(
                    "{}{} r{} @{}",
                    r["axis"].as_str().unwrap(),
                    if r["turned"].as_bool().unwrap() {
                        "T"
                    } else {
                        ""
                    },
                    r["radius"],
                    r["at"]
                )
            };
            let g: Vec<String> = got.iter().map(brief).collect();
            let w: Vec<String> = want.iter().map(brief).collect();
            let only_rust: Vec<&String> = g.iter().filter(|x| !w.contains(x)).collect();
            let only_python: Vec<&String> = w.iter().filter(|x| !g.contains(x)).collect();
            problems.push(format!(
                "{name} {}: {} records vs Python {}; only rust {only_rust:?}; only python {only_python:?}",
                run["options"],
                g.len(),
                w.len()
            ));
            continue;
        }
        match (
            discover_verified(&Context::new(part), &opts),
            run.get("evidence_error"),
        ) {
            (Err(_), Some(_)) => {}
            (Ok(found), None) => {
                let faces: Vec<u64> = found.iter().map(|o| o.defining[0] as u64).collect();
                let want: Vec<u64> = run["defining"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| d["face"].as_u64().unwrap())
                    .collect();
                if faces != want {
                    problems.push(format!(
                        "{name} {}: defining faces {faces:?}, Python {want:?}",
                        run["options"]
                    ));
                }
            }
            (got, want) => problems.push(format!(
                "{name} {}: evidence {got:?}, Python {want:?}",
                run["options"]
            )),
        }
    }
}

#[test]
fn built_fixtures_match_python() {
    let manifest = manifest();
    let mut problems = Vec::new();
    for entry in manifest["built"].as_array().unwrap() {
        let name = entry["name"].as_str().unwrap();
        let part = read_step_file(&fixtures().join(entry["file"].as_str().unwrap())).unwrap();
        common::check_inventory(
            name,
            &part,
            entry["inventory"].as_array().unwrap(),
            &mut problems,
        );
        check_runs(
            name,
            &part,
            entry["runs"].as_array().unwrap(),
            &mut problems,
        );
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// Gusset-rib evidence on the Python suite's ribs (`tests/test_gussets.py`): a sharp rib claims
/// its two caps and slant; rounding one or both hypotenuse edges adds each blend; a hollow rib
/// fails the material probe after its caps match.
#[test]
fn gusset_rib_evidence_claims_caps_slant_and_blends() {
    use quiddity::features::gussets;
    let captured = fixtures().join("captured");
    for (file, want) in [
        ("05eb6351f4106b7e.step.gz", Some(3)),
        ("a96691391681646d.step.gz", Some(4)),
        ("9d0b60cef180d03e.step.gz", Some(5)),
        ("6783fb0ec976fa04.step.gz", None),
    ] {
        let part = read_step_file(&captured.join(file)).unwrap();
        let found = gussets::discover_verified(&Context::new(&part)).unwrap();
        let got = match found.as_slice() {
            [] => None,
            [one] => Some(one.defining.len()),
            more => panic!("{file}: {} ribs", more.len()),
        };
        eprintln!("{file}: {got:?}");
        assert_eq!(got, want, "{file}");
    }
}

/// Double-D bore evidence on every part the Python suite finds bores in: each bore claims its
/// four lateral wall faces (two flats, two arcs), as Python's ledger records them
/// (`_discover_double_d_bores(part, writer=...)` on the re-read STEP). Python refuses
/// c20163878470d13f: its bore's walls have no one valid owner solid.
#[test]
fn double_d_bore_evidence_claims_the_lateral_walls() {
    use quiddity::features::profiled_bores;
    let captured = fixtures().join("captured");
    let one: &[&[usize]] = &[&[6, 7, 8, 9]];
    let two: &[&[usize]] = &[&[6, 7, 8, 9], &[16, 17, 18, 19]];
    for (file, want) in [
        ("0d84b2b05e79d24b.step.gz", Some(one)),
        ("1a56fe5ea5c63237.step.gz", Some(one)),
        ("3f53e72fd9b3e1bc.step.gz", Some(two)),
        ("489db46ea4efc3dd.step.gz", Some(one)),
        ("571fd1c6434b6881.step.gz", Some(one)),
        ("596778d0ee391065.step.gz", Some(one)),
        ("6bfbd534bb77b12c.step.gz", Some(one)),
        ("8a7e4f9e215b62d4.step.gz", Some(two)),
        ("8f54543dec08d720.step.gz", Some(one)),
        ("b7372b070eb84b7f.step.gz", Some(two)),
        ("c20163878470d13f.step.gz", None),
        ("c7ee6c53ea0011e7.step.gz", Some(one)),
        ("cf995c7697933eb2.step.gz", Some(one)),
        ("d76302e36734c006.step.gz", Some(one)),
        ("ecfc67b700d16cd1.step.gz", Some(one)),
        ("fdd884eb448f88e9.step.gz", Some(one)),
        ("fddbb89b8c56156c.step.gz", Some(one)),
    ] {
        let want: Option<Vec<Vec<usize>>> = want.map(|w| w.iter().map(|d| d.to_vec()).collect());
        // The corpus has no double-D bores, so the invariance test cannot exercise them: the
        // same walls must also be found with the part moved and its axes cycled.
        for (motion, placement) in [
            ("unmoved", IDENTITY),
            (
                "cycled and moved",
                [
                    [0.0, 0.0, 1.0, 123.456],
                    [1.0, 0.0, 0.0, -78.9],
                    [0.0, 1.0, 0.0, 41.3],
                ],
            ),
        ] {
            let part = read_step_file_placed(&captured.join(file), &placement).unwrap();
            let mut got: Option<Vec<Vec<usize>>> =
                profiled_bores::discover_verified(&Context::new(&part))
                    .ok()
                    .map(|found| {
                        found
                            .into_iter()
                            .map(|o| {
                                let mut d = o.defining;
                                d.sort_unstable();
                                d
                            })
                            .collect()
                    });
            if let Some(g) = got.as_mut() {
                g.sort();
            }
            assert_eq!(got, want, "{file} {motion}");
        }
    }
}

/// Edge-open recess evidence on the Python suite's parts (`tests/test_edge_open_*`): the circular
/// pocket claims its four walls and the hexagonal recess its six, each with its floor consulted
/// (Python's constituent of five and seven); the two recesses of a two-solid compound are claimed
/// each within its own solid.
#[test]
fn edge_open_recess_evidence_claims_walls_and_consults_the_floor() {
    use quiddity::features::evidence::common_valid_solid;
    use quiddity::features::{edge_open_circular, edge_open_prismatic};
    let captured = fixtures().join("captured");
    let read = |file: &str| read_step_file(&captured.join(file)).unwrap();
    let circular = read("fa0600fbcc4940d4.step.gz");
    let found = edge_open_circular::discover_verified(&Context::new(&circular)).unwrap();
    let [pocket] = found.as_slice() else {
        panic!("{} circular pockets", found.len())
    };
    assert_eq!((pocket.defining.len(), pocket.context.len()), (4, 1));
    let hexagon = read("10031362cc38ab85.step.gz");
    let found = edge_open_prismatic::discover_verified(&Context::new(&hexagon)).unwrap();
    let [recess] = found.as_slice() else {
        panic!("{} prismatic recesses", found.len())
    };
    assert_eq!((recess.defining.len(), recess.context.len()), (6, 1));
    let compound = read("da68caa0e2caaa0d.step.gz");
    let found = edge_open_prismatic::discover_verified(&Context::new(&compound)).unwrap();
    let solids: Vec<Option<usize>> = found
        .iter()
        .map(|o| {
            let faces: Vec<usize> = o.defining.iter().chain(&o.context).copied().collect();
            common_valid_solid(&compound, &faces)
        })
        .collect();
    assert_eq!(solids.len(), 2);
    assert!(solids.iter().all(Option::is_some) && solids[0] != solids[1]);
}

/// A floor with a hole through it still proves the pocket (Python's `Solid.extrude` sweeps the
/// hole's closed rim to a cylinder band). Built with build123d: a 12-high plate (pentagon
/// (-30,-20), (30,-20), (30,10), (20,20), (-30,20)) less a 30 x 10 `SlotOverall` at (16, 10)
/// from z = 4 up, less a 3 mm hole through the floor at (12, 10). Python gives this one record.
#[test]
fn edge_open_pocket_with_a_hole_through_its_floor() {
    use quiddity::features::edge_open_circular::recognise_edge_open_circular_pockets;
    let part = read_step_file(&fixtures().join("edge_open_pocket_floor_hole.step")).unwrap();
    let found = recognise_edge_open_circular_pockets(&part);
    let [pocket] = found.as_slice() else {
        panic!("{} circular pockets", found.len())
    };
    let got = serde_json::to_value(pocket).unwrap();
    assert_eq!(got["axis"], "z");
    assert_eq!(got["run_interval"], serde_json::json!([4.0, 12.0]));
    assert_eq!(got["open_sign"], 1);
    assert_eq!(
        got["section"]["opening"],
        serde_json::json!([[25.0, 15.0], [30.0, 7.0]])
    );
    assert_eq!(got["section"]["segments"].as_array().unwrap().len(), 4);
}
