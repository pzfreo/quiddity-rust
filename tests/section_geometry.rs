//! The cylindrical proofs section-recess geometry composes against Python's answers
//! (`tools/capture_section_geometry.py`, fixtures in `tests/fixtures/captured/section_geometry/`):
//! `_cylindrical_channels`, `_cylindrical_pockets` and `_cylindrical_passages` have no
//! `recognise_*` entry point, so their calls are replayed directly.
//!
//! `proofs.json.gz` holds, per part (the parts the Python tests hand these proofs, the golden
//! fixtures and the corpus), `cylindrical_pocket_proofs` with every `_proofs(floor)` question,
//! `cylindrical_passage_proofs` with the native bores and every `_cell_proof` question, and every
//! `prove_cylindrical_channel` question (on a test part those its tests asked, on a fixture or
//! corpus part those Python's recognition asks), each replayed on its own.
//!
//! Floats agree to one part in a million (`common::same`). Python orders passage proofs by its
//! unspecified component walk, so a part's passage proofs are compared sorted by their faces.
//! Differences are listed in `tests/fixtures/captured/known_section_geometry.json`: per entry its
//! `case` (`pockets`, `passages`, `bores`, `pocket_call`, `passage_call`, `channel_call`), what
//! identifies it (a part's `file`, a pocket call's `floor`, a passage call's `planar`,
//! `cylinder` and `walls`, a channel call's `defining`), a verdict and a reason. A failing run
//! prints the entries it needs.
//!
//! The proofs are also checked under `tests/invariance.rs`'s rigid motions: every part on which
//! Python proves something is re-read moved, and must prove the same on the same faces, with the
//! same placement-free values (`case` `invariance`, identified by `file` and `motion`).

mod common;
#[macro_use]
#[path = "support/slices.rs"]
mod slices;

use std::collections::BTreeSet;
use std::io::Read;

use quiddity::Part;
use quiddity::features::Context;
use quiddity::features::cylindrical_channels::{CylindricalChannelProof, prove as prove_channel};
use quiddity::features::cylindrical_passages::{
    CylindricalPassageProof, cylindrical_passage_proofs, native_bores, prove as prove_passage,
};
use quiddity::features::cylindrical_pockets::{
    CylindricalPocketProof, cylindrical_pocket_proofs, proofs as pocket_proofs,
};
use quiddity::features::effective_surfaces::EffectiveFaces;
use quiddity::features::sections::LocalFrame;
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const KNOWN: &str = "captured/known_section_geometry.json";

fn dir() -> std::path::PathBuf {
    common::fixtures().join("captured/section_geometry")
}

fn load_gz(name: &str) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(dir().join(name)).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn floats<const N: usize>(v: &Value) -> [f64; N] {
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), N, "{v}");
    std::array::from_fn(|i| a[i].as_f64().unwrap())
}

fn indices(v: &Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as usize)
        .collect()
}

fn frame_json(f: &LocalFrame) -> Value {
    json!({"origin": f.origin, "run": f.run, "u": f.u, "v": f.v})
}

fn channel_json(p: &CylindricalChannelProof) -> Value {
    json!({
        "supports": p.supports,
        "cylinder": p.cylinder,
        "planar_context": p.planar_context,
        "owner": p.owner,
        "run_axis": p.run_axis,
        "width_axis": p.width_axis,
        "open_sign": p.open_sign,
        "bounds": p.bounds.iter().map(|b| [b.0, b.1]).collect::<Vec<_>>(),
        "run_interval": [p.run_interval.0, p.run_interval.1],
        "cylindrical_end": p.cylindrical_end,
        "axis_point": p.axis_point,
        "axis_direction": p.axis_direction,
        "radius": p.radius,
        "volume": p.volume,
    })
}

fn pocket_json(p: &CylindricalPocketProof) -> Value {
    json!({
        "floor": p.floor,
        "walls": p.walls,
        "stock": p.stock,
        "owner": p.owner,
        "axis_point": p.axis_point,
        "axis_direction": p.axis_direction,
        "run": p.run,
        "radius": p.radius,
        "volume": p.volume,
    })
}

fn passage_json(p: &CylindricalPassageProof) -> Value {
    json!({
        "walls": p.walls,
        "cylinder": p.cylinder,
        "planar_context": p.planar_context,
        "owner": p.owner,
        "frame": frame_json(&p.frame),
        "section": p.section.boundary().iter()
            .map(|v| [v.point[0], v.point[1], v.bulge]).collect::<Vec<_>>(),
        "run_interval": [p.run_interval.0, p.run_interval.1],
        "cylindrical_end": p.cylindrical_end,
        "axis_point": p.axis_point,
        "axis_direction": p.axis_direction,
        "radius": p.radius,
        "volume": p.volume,
    })
}

/// A part's passage proofs sorted by their faces (Python's order follows its unspecified
/// component walk).
fn sorted(proofs: &Value) -> Value {
    let Some(list) = proofs.as_array() else {
        return proofs.clone();
    };
    let mut list = list.clone();
    let key = |p: &Value| {
        (
            p["planar_context"].as_u64(),
            p["cylinder"].as_u64(),
            indices(&p["walls"]),
        )
    };
    list.sort_by_key(key);
    Value::Array(list)
}

/// Checks *found* differences (identity, description) against the listed entries of *case*, and
/// fails on an unlisted difference or a listed entry that no longer differs (of the files *ran*:
/// without the corpus its entries are not checked).
fn check_known(case: &str, found: Vec<(Value, String)>, ran: &BTreeSet<String>) {
    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts("known_section_geometry.json", known);
    let listed: Vec<&Value> = known
        .iter()
        .filter(|k| k["case"] == case)
        .filter(|k| k["file"].as_str().is_some_and(|f| ran.contains(f)))
        .collect();
    let mut problems = Vec::new();
    let mut seen = vec![false; listed.len()];
    for (identity, text) in &found {
        let position = listed.iter().position(|k| {
            identity
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| &k[key] == value)
        });
        match position {
            Some(i) if seen[i] => problems.push(format!("listed twice: {identity}")),
            Some(i) => seen[i] = true,
            None => problems.push(format!("unlisted difference: {text}\n  entry: {identity}")),
        }
    }
    for (i, k) in listed.iter().enumerate() {
        if !seen[i] {
            problems.push(format!("listed entry no longer differs: {k}"));
        }
    }
    assert!(problems.is_empty(), "{case}:\n{}", problems.join("\n"));
}

fn path(source: &str, file: &str) -> Option<std::path::PathBuf> {
    Some(match source {
        "test" => dir().join(file),
        "fixture" => common::fixtures().join(file),
        _ => common::corpus_dir()?.join(file),
    })
}

fn read(source: &str, file: &str) -> Option<Part> {
    let path = path(source, file)?;
    Some(read_step_file(&path).unwrap_or_else(|e| panic!("{file}: {e}")))
}

/// What one part's replay found.
#[derive(Default)]
struct Tally {
    pocket_calls: usize,
    passage_calls: usize,
    channel_calls: usize,
    /// Channel questions the capture could not answer (a timed-out or refused recognition).
    uncaptured: usize,
    found: Vec<(Value, String)>,
}

/// The port's answer to one captured channel question.
fn channel_answer(ctx: &Context<'_>, surfaces: &EffectiveFaces<'_, '_>, call: &Value) -> Value {
    let set = |v: &Value| indices(v).into_iter().collect::<BTreeSet<usize>>();
    prove_channel(
        ctx,
        surfaces,
        &set(&call["defining"]),
        &set(&call["constituent"]),
        call["run_axis"].as_str().unwrap(),
        call["width_axis"].as_str().unwrap(),
        call["open_sign"].as_i64().unwrap() as i32,
    )
    .map_or(Value::Null, |p| channel_json(&p))
}

#[test]
fn cylindrical_proofs_agree_with_python() {
    let captured = load_gz("proofs.json.gz");
    let runs: Vec<&Value> = captured["runs"].as_array().unwrap().iter().collect();
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
    let ran: BTreeSet<String> = runs
        .iter()
        .filter(|r| corpus.is_some() || r["source"] != "corpus")
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect();
    let tallies = common::parallel::map(&runs, |run| {
        let file = run["file"].as_str().unwrap();
        let mut tally = Tally::default();
        let Some(part) = read(run["source"].as_str().unwrap(), file) else {
            return tally;
        };
        let ctx = Context::new(&part);
        let surfaces = EffectiveFaces::new(&ctx);
        let pockets = json!(
            cylindrical_pocket_proofs(&ctx, &surfaces)
                .iter()
                .map(pocket_json)
                .collect::<Vec<_>>()
        );
        let passages = json!(
            cylindrical_passage_proofs(&ctx, &surfaces)
                .iter()
                .map(passage_json)
                .collect::<Vec<_>>()
        );
        let bores = json!(native_bores(&ctx, &surfaces));
        for (case, got) in [
            ("pockets", pockets),
            ("passages", passages),
            ("bores", bores),
        ] {
            let (got, want) = if case == "passages" {
                (sorted(&got), sorted(&run[case]))
            } else {
                (got, run[case].clone())
            };
            if !common::same(&got, &want) {
                tally.found.push((
                    json!({"case": case, "file": file}),
                    format!("{file} {case}: {}", common::diff(&got, &want)),
                ));
            }
        }
        for call in run["pocket_calls"].as_array().unwrap() {
            tally.pocket_calls += 1;
            let floor = call["floor"].as_u64().unwrap() as usize;
            let got = json!(
                pocket_proofs(&ctx, &surfaces, floor)
                    .iter()
                    .map(pocket_json)
                    .collect::<Vec<_>>()
            );
            if !common::same(&got, &call["proofs"]) {
                tally.found.push((
                    json!({"case": "pocket_call", "file": file, "floor": floor}),
                    format!(
                        "{file} pocket floor {floor}: {}",
                        common::diff(&got, &call["proofs"])
                    ),
                ));
            }
        }
        for call in run["passage_calls"].as_array().unwrap() {
            tally.passage_calls += 1;
            let walls = indices(&call["walls"]);
            let planar = call["planar"].as_u64().unwrap() as usize;
            let cylinder = call["cylinder"].as_u64().unwrap() as usize;
            let b = &call["base"];
            let base = LocalFrame::new(
                floats::<3>(&b["origin"]),
                floats::<3>(&b["run"]),
                floats::<3>(&b["u"]),
                floats::<3>(&b["v"]),
            )
            .unwrap();
            let got = prove_passage(&ctx, &surfaces, planar, cylinder, &walls, &base)
                .map_or(Value::Null, |p| passage_json(&p));
            if !common::same(&got, &call["proof"]) {
                tally.found.push((
                    json!({"case": "passage_call", "file": file, "planar": planar,
                           "cylinder": cylinder, "walls": walls}),
                    format!(
                        "{file} passage {planar} {cylinder} {walls:?}: {}",
                        common::diff(&got, &call["proof"])
                    ),
                ));
            }
        }
        match run["channel_calls"].as_array() {
            None => tally.uncaptured += 1,
            Some(calls) => {
                for call in calls {
                    if call.get("refused").is_some() {
                        tally.uncaptured += 1;
                        continue;
                    }
                    tally.channel_calls += 1;
                    let got = channel_answer(&ctx, &surfaces, call);
                    if !common::same(&got, &call["proof"]) {
                        tally.found.push((
                            json!({"case": "channel_call", "file": file,
                                   "defining": call["defining"]}),
                            format!(
                                "{file} channel {}: {}",
                                call["defining"],
                                common::diff(&got, &call["proof"])
                            ),
                        ));
                    }
                }
            }
        }
        tally
    });
    let count = |f: fn(&Tally) -> usize| tallies.iter().map(f).sum::<usize>();
    let (pocket_calls, passage_calls, channel_calls, uncaptured) = (
        count(|t| t.pocket_calls),
        count(|t| t.passage_calls),
        count(|t| t.channel_calls),
        count(|t| t.uncaptured),
    );
    let found: Vec<(Value, String)> = tallies.into_iter().flat_map(|t| t.found).collect();
    let mut problems = Vec::new();
    for case in [
        "pockets",
        "passages",
        "bores",
        "pocket_call",
        "passage_call",
        "channel_call",
    ] {
        let mine: Vec<_> = found
            .iter()
            .filter(|(identity, _)| identity["case"] == case)
            .cloned()
            .collect();
        eprintln!("{case}: {} differ", mine.len());
        if let Err(e) = std::panic::catch_unwind(|| check_known(case, mine, &ran)) {
            problems.push(
                e.downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| "check failed".into()),
            );
        }
    }
    eprintln!(
        "{} parts, {pocket_calls} pocket calls, {passage_calls} passage calls, {channel_calls} \
         channel calls replayed, {uncaptured} channel runs or calls not captured",
        ran.len()
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// `tests/invariance.rs`'s motions: a non-round translation and right-angle rotations.
const T: [f64; 3] = [123.456, -78.9, 41.3];
/// A named motion: rotation rows and translation.
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);
const MOTIONS: [Motion; 6] = [
    (
        "translate",
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        T,
    ),
    (
        "rot_z90",
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0; 3],
    ),
    (
        "rot_x90",
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_y180",
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0; 3],
    ),
    (
        "cycle_xyz",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_zx_moved",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        T,
    ),
];

const AXES: [&str; 3] = ["x", "y", "z"];

/// Where a right-angle rotation takes coordinate axis *name*: the axis and the sign.
fn turned_axis(rotation: &[[f64; 3]; 3], name: &str) -> (&'static str, i64) {
    let i = AXES.iter().position(|&a| a == name).unwrap();
    let j = (0..3).find(|&j| rotation[j][i] != 0.0).unwrap();
    (AXES[j], rotation[j][i].signum() as i64)
}

/// What a part proves independent of where it sits: per proof its faces and its placement-free
/// values, sorted, and each captured channel question (its axes turned with the part) with its
/// faces and values.
fn placement_free(part: &Part, channel_calls: &[Value], rotation: &[[f64; 3]; 3]) -> Value {
    let ctx = Context::new(part);
    let surfaces = EffectiveFaces::new(&ctx);
    let mut pockets: Vec<Value> = cylindrical_pocket_proofs(&ctx, &surfaces)
        .iter()
        .map(|p| {
            json!({
                "floor": p.floor, "walls": p.walls, "stock": p.stock, "owner": p.owner,
                "radius": p.radius, "volume": p.volume,
            })
        })
        .collect();
    pockets.sort_by_key(|p| (p["floor"].as_u64(), p["stock"].as_u64()));
    let mut passages: Vec<Value> = cylindrical_passage_proofs(&ctx, &surfaces)
        .iter()
        .map(|p| {
            json!({
                "walls": p.walls, "cylinder": p.cylinder, "planar_context": p.planar_context,
                "owner": p.owner, "area": p.section.area(),
                "sides": p.section.boundary().len(),
                "length": (p.run_interval.1 - p.run_interval.0).abs(),
                "radius": p.radius, "volume": p.volume,
            })
        })
        .collect();
    passages.sort_by_key(|p| {
        (
            p["planar_context"].as_u64(),
            p["cylinder"].as_u64(),
            indices(&p["walls"]),
        )
    });
    let channels: Vec<Value> = channel_calls
        .iter()
        .map(|call| {
            let (run_axis, _) = turned_axis(rotation, call["run_axis"].as_str().unwrap());
            let (width_axis, _) = turned_axis(rotation, call["width_axis"].as_str().unwrap());
            let depth = AXES
                .iter()
                .find(|&&a| {
                    a != call["run_axis"].as_str().unwrap()
                        && a != call["width_axis"].as_str().unwrap()
                })
                .unwrap();
            let (_, sign) = turned_axis(rotation, depth);
            let mut turned = call.clone();
            turned["run_axis"] = json!(run_axis);
            turned["width_axis"] = json!(width_axis);
            turned["open_sign"] = json!(call["open_sign"].as_i64().unwrap() * sign);
            match channel_answer(&ctx, &surfaces, &turned) {
                Value::Null => Value::Null,
                p => json!({
                    "supports": p["supports"], "cylinder": p["cylinder"],
                    "planar_context": p["planar_context"], "owner": p["owner"],
                    "radius": p["radius"], "volume": p["volume"],
                    "length": (p["run_interval"][1].as_f64().unwrap()
                        - p["run_interval"][0].as_f64().unwrap()).abs(),
                }),
            }
        })
        .collect();
    json!({"pockets": pockets, "passages": passages, "channels": channels})
}

fn proved_call(c: &Value) -> bool {
    !c["proof"].is_null() && c.get("proof").is_some()
}

/// The runs on which Python proves something: those moved by
/// [`proofs_do_not_depend_on_placement`].
fn moved_runs(captured: &Value) -> Vec<&Value> {
    captured["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            ["pockets", "passages"]
                .iter()
                .any(|k| r[k].as_array().is_some_and(|a| !a.is_empty()))
                || r["channel_calls"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(proved_call))
        })
        .collect()
}

/// The files of the moved runs, each once, in run order: what the invariance test is sliced by,
/// so every run of a file (and every listed entry of it) is in one slice.
fn moved_files() -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for run in moved_runs(&load_gz("proofs.json.gz")) {
        let file = run["file"].as_str().unwrap();
        if !files.iter().any(|f| f == file) {
            files.push(file.to_string());
        }
    }
    files
}

/// The proofs under every motion: one test per slice of the moved runs' files
/// (`tests/support/slices.rs`).
fn proofs_invariant(k: usize, n: usize) {
    let captured = load_gz("proofs.json.gz");
    let all = moved_runs(&captured);
    assert!(!all.is_empty());
    let mine = slices::slice(&moved_files(), k, n);
    let runs: Vec<&Value> = all
        .into_iter()
        .filter(|r| mine.iter().any(|f| r["file"] == f.as_str()))
        .collect();
    let found: Vec<(Value, String)> = common::parallel::map(&runs, |run| {
        let file = run["file"].as_str().unwrap();
        let source = run["source"].as_str().unwrap();
        let Some(path) = path(source, file) else {
            return Vec::new();
        };
        let Some(part) = read(source, file) else {
            return Vec::new();
        };
        let calls: Vec<Value> = run["channel_calls"]
            .as_array()
            .map(|a| a.iter().filter(|c| proved_call(c)).cloned().collect())
            .unwrap_or_default();
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let want = placement_free(&part, &calls, &identity);
        let mut out = Vec::new();
        for (name, r, t) in MOTIONS {
            let placement: Placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]]);
            let moved =
                read_step_file_placed(&path, &placement).unwrap_or_else(|e| panic!("{file}: {e}"));
            let got = placement_free(&moved, &calls, &r);
            if !common::same(&got, &want) {
                out.push((
                    json!({"case": "invariance", "file": file, "motion": name}),
                    format!(
                        "{file} {name}: moved, unmoved {}",
                        common::diff(&got, &want)
                    ),
                ));
            }
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    let ran: BTreeSet<String> = runs
        .iter()
        .filter(|r| common::corpus_dir().is_some() || r["source"] != "corpus")
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect();
    check_known("invariance", found, &ran);
}

sliced!(
    proofs_do_not_depend_on_placement,
    super::proofs_invariant,
    files: super::moved_files,
    [0 => slice_0, 1 => slice_1, 2 => slice_2]
);
