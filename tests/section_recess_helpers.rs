//! The section-recess helpers against Python's answers (`tools/capture_section_recess_helpers.py`,
//! fixtures in `tests/fixtures/captured/section_recess_helpers/`): `_cylindrical_seats`,
//! `_cylindrical_end_surface` and `_plane_envelope_passages` have no `recognise_*` entry point,
//! so their calls are replayed directly.
//!
//! - `values.json.gz`: every `CylindricalEndSurface` the Python tests build, and every `height`
//!   and `polygon_height_bounds` question asked of one, with its answer or its refusal (the same
//!   message);
//! - `proofs.json.gz`: `cylindrical_seat_proofs` and `plane_envelope_passage_proofs` on the parts
//!   the Python tests hand them, the golden fixtures and the corpus, and every private `_prove`
//!   question asked on the way, each replayed on its own. The test parts include a sample of
//!   those on which Python proves nothing (the rest are counted as `unselected`), where the port
//!   must prove nothing too.
//!
//! Floats agree to one part in a million (`common::same`), refusals exactly. Python orders
//! envelope proofs by its unspecified component walk, so a part's proofs are compared sorted by
//! their faces, and traces a seat's arc in a direction that changes from run to run, so seats
//! are compared in one direction (`seat_direction`). Differences are listed in
//! `tests/fixtures/captured/known_section_recess_helpers.json`: per entry its `case` (`value`,
//! `seats`, `envelopes`, `seat_call`, `envelope_call`), what identifies it (a value's `kind` and
//! `given` input, a part's `file`, a call's `file` and `walls`, and an envelope call's `mouth`),
//! a verdict and a reason. A failing run prints the entries it needs.
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
use quiddity::features::cylindrical_end_surface::CylindricalEndSurface;
use quiddity::features::cylindrical_seats::{
    CylindricalSeatProof, SeatCylinder, cylindrical_seat_proofs, prove as prove_seat,
};
use quiddity::features::plane_envelope_passages::{
    PlaneEnvelopePassageProof, plane_envelope_passage_proofs, prove as prove_envelope,
};
use quiddity::features::sections::{LocalFrame, SectionError, SectionVertex};
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const KNOWN: &str = "captured/known_section_recess_helpers.json";

fn dir() -> std::path::PathBuf {
    common::fixtures().join("captured/section_recess_helpers")
}

fn load_gz(name: &str) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(dir().join(name)).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// A captured float: a number, or a non-finite one written as its Python string.
fn float(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "nan" => f64::NAN,
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            _ => panic!("not a float: {s}"),
        },
        _ => panic!("not a float: {v}"),
    }
}

fn floats<const N: usize>(v: &Value) -> [f64; N] {
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), N, "{v}");
    std::array::from_fn(|i| float(&a[i]))
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

fn vertex_json(vs: &[SectionVertex]) -> Value {
    json!(
        vs.iter()
            .map(|v| [v.point[0], v.point[1], v.bulge])
            .collect::<Vec<_>>()
    )
}

fn seat_json(p: &CylindricalSeatProof) -> Value {
    json!({
        "walls": p.walls,
        "context": p.context,
        "owner": p.owner,
        "frame": frame_json(&p.frame),
        "boundary": vertex_json(&p.boundary),
        "run_interval": [p.run_interval.0, p.run_interval.1],
    })
}

fn envelope_json(p: &PlaneEnvelopePassageProof) -> Value {
    json!({
        "walls": p.walls,
        "planar_context": p.planar_context,
        "roof_contexts": p.roof_contexts,
        "owner": p.owner,
        "frame": frame_json(&p.frame),
        "section": vertex_json(p.section.boundary()),
        "run_interval": [p.run_interval.0, p.run_interval.1],
        "envelope_end": p.envelope_end,
        "terms": p.terms.iter().map(|(h, g)| json!([h, g])).collect::<Vec<_>>(),
        "volume": p.volume,
    })
}

/// A captured surface's fields, built by the port.
fn surface(given: &Value) -> Result<CylindricalEndSurface, SectionError> {
    CylindricalEndSurface::new(
        given["type"].as_str().unwrap(),
        floats::<3>(&given["axis_point"]),
        floats::<2>(&given["axis_direction"]),
        float(&given["radius"]),
        given["branch"].as_str().unwrap(),
    )
}

/// The port's answer to one captured value call, in the capture's shape.
fn value_answer(kind: &str, given: &Value) -> Value {
    let refused = |e: SectionError| json!({"refused": e.0});
    match kind {
        "surface" => match surface(given) {
            Ok(s) => json!({"accepted": true, "dict": serde_json::to_value(&s).unwrap()}),
            Err(e) => refused(e),
        },
        "height" => {
            match surface(&given["surface"]).and_then(|s| s.height(floats::<2>(&given["point"]))) {
                Ok(h) => json!({"height": h}),
                Err(e) => refused(e),
            }
        }
        "bounds" => {
            let points: Vec<[f64; 2]> = given["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(floats::<2>)
                .collect();
            match surface(&given["surface"]).and_then(|s| s.polygon_height_bounds(&points)) {
                Ok((lo, hi)) => json!({"bounds": [lo, hi]}),
                Err(e) => refused(e),
            }
        }
        _ => panic!("unknown value kind {kind}"),
    }
}

/// What a captured record answered, without its identifying input.
fn answer_of(record: &Value) -> Value {
    let mut answer = record.clone();
    let map = answer.as_object_mut().unwrap();
    map.remove("kind");
    map.remove("given");
    answer
}

/// Checks *found* differences (identity, description) against the listed entries of *case*, and
/// fails on an unlisted difference or a listed entry that no longer differs (of the files *ran*,
/// when given: without the corpus its entries are not checked).
fn check_known(case: &str, found: Vec<(Value, String)>, ran: Option<&BTreeSet<String>>) {
    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts("known_section_recess_helpers.json", known);
    let listed: Vec<&Value> = known
        .iter()
        .filter(|k| k["case"] == case)
        .filter(|k| ran.is_none_or(|r| k["file"].as_str().is_some_and(|f| r.contains(f))))
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

#[test]
fn end_surface_values_agree_with_python() {
    let captured = load_gz("values.json.gz");
    let values = captured["values"].as_array().unwrap();
    assert!(!values.is_empty());
    let mut found = Vec::new();
    for record in values {
        let kind = record["kind"].as_str().unwrap();
        let got = value_answer(kind, &record["given"]);
        let want = answer_of(record);
        if !common::same(&got, &want) {
            found.push((
                json!({"case": "value", "kind": kind, "given": record["given"]}),
                format!("{kind} {}: {}", record["given"], common::diff(&got, &want)),
            ));
        }
    }
    eprintln!(
        "end-surface values: {} captured, {} differ, {} skipped by the capture",
        values.len(),
        found.len(),
        captured["skipped"]
    );
    check_known("value", found, None);
}

/// An envelope proof with its two roof terms in gradient order when their heights agree to
/// round-off (`common::same`). The terms are sorted by height, then gradient, so two roofs
/// through one point of the frame's run (any symmetric roof) are ordered by the last bit of
/// their heights, which each side reads through a different point on the roof plane (Python
/// the face's centroid, the port the plane's own origin).
fn tied_terms(proof: &Value) -> Value {
    let mut proof = proof.clone();
    if let Some(terms) = proof.get_mut("terms").and_then(Value::as_array_mut)
        && terms.len() == 2
        && common::same(&terms[0][0], &terms[1][0])
    {
        let key = |t: &Value| floats::<2>(&t[1]);
        terms.sort_by(|a, b| {
            let (a, b) = (key(a), key(b));
            a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1]))
        });
    }
    proof
}

/// A seat proof with its arc traced in the direction of positive bulge. Python takes the arc's
/// first and last vertex from the order a dict of rim vertices was filled in from a `set[Edge]`,
/// whose order follows OCCT's shape hash (`hash(edge.wrapped)`), which changes from process to
/// process even with `PYTHONHASHSEED` fixed: on `parts/bb776cefab5303f2.step.gz` two captures
/// with `PYTHONHASHSEED=0` gave the boundary `[(-1.317, 3.515, 0.282), (2.070, -3.131, 0)]` and
/// the same arc reversed, `[(2.070, -3.131, -0.282), (-1.317, 3.515, 0)]`. The direction is not
/// a property of the part, so both sides are compared in one direction; the vertices and the arc
/// are still compared. A recapture therefore does not reproduce the fixture byte for byte (the
/// order of `_prove` questions moves the same way), only its content up to this direction and
/// order.
fn seat_direction(proof: &Value) -> Value {
    let mut proof = proof.clone();
    let Some(bulge) = proof["boundary"]
        .as_array()
        .filter(|b| b.len() == 2)
        .map(|b| float(&b[0][2]))
    else {
        return proof;
    };
    // A seat's arc has a non-zero bulge, so it has a direction to normalise.
    assert_ne!(bulge, 0.0, "seat arc without a bulge: {proof}");
    if bulge < 0.0 {
        let boundary = proof["boundary"].as_array_mut().unwrap();
        let (first, last) = (floats::<3>(&boundary[1]), floats::<3>(&boundary[0]));
        *boundary = vec![
            json!([first[0], first[1], -last[2]]),
            json!([last[0], last[1], 0.0]),
        ];
    }
    proof
}

/// A part's proofs sorted by their faces (Python's envelope order follows its unspecified
/// component walk), each with its tied terms ordered and a seat's arc in one direction.
fn sorted(proofs: &Value) -> Value {
    let Some(list) = proofs.as_array() else {
        return proofs.clone();
    };
    let mut list: Vec<Value> = list
        .iter()
        .map(tied_terms)
        .map(|p| seat_direction(&p))
        .collect();
    let key = |p: &Value| (p["planar_context"].as_u64(), indices(&p["walls"]));
    list.sort_by_key(key);
    Value::Array(list)
}

fn read(source: &str, file: &str) -> Option<Part> {
    let path = match source {
        "test" => dir().join(file),
        "fixture" => common::fixtures().join(file),
        _ => common::corpus_dir()?.join(file),
    };
    Some(read_step_file(&path).unwrap_or_else(|e| panic!("{file}: {e}")))
}

/// What one call replay found: a difference, or a match, per module.
#[derive(Default)]
struct Tally {
    seat_calls: usize,
    envelope_calls: usize,
    found: Vec<(Value, String)>,
}

#[test]
fn seat_and_envelope_proofs_agree_with_python() {
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
        let seats = json!(
            cylindrical_seat_proofs(&ctx)
                .iter()
                .map(seat_json)
                .collect::<Vec<_>>()
        );
        let envelopes = json!(
            plane_envelope_passage_proofs(&ctx)
                .iter()
                .map(envelope_json)
                .collect::<Vec<_>>()
        );
        for (case, got) in [("seats", seats), ("envelopes", envelopes)] {
            let (got, want) = (sorted(&got), sorted(&run[case]));
            if !common::same(&got, &want) {
                tally.found.push((
                    json!({"case": case, "file": file}),
                    format!("{file} {case}: {}", common::diff(&got, &want)),
                ));
            }
        }
        for call in run["seat_calls"].as_array().unwrap() {
            tally.seat_calls += 1;
            let walls = indices(&call["walls"]);
            let cylinder = SeatCylinder {
                radius: float(&call["radius"]),
                axis: floats::<3>(&call["axis"]),
                origin: floats::<3>(&call["origin"]),
            };
            let got = prove_seat(&ctx, &walls, &cylinder).map_or(Value::Null, |p| seat_json(&p));
            if !common::same(&seat_direction(&got), &seat_direction(&call["proof"])) {
                tally.found.push((
                    json!({"case": "seat_call", "file": file, "walls": walls}),
                    format!(
                        "{file} seat {walls:?}: {}",
                        common::diff(&got, &call["proof"])
                    ),
                ));
            }
        }
        for call in run["envelope_calls"].as_array().unwrap() {
            tally.envelope_calls += 1;
            let walls = indices(&call["walls"]);
            let mouth = call["mouth"].as_u64().unwrap() as usize;
            let b = &call["base"];
            let base = LocalFrame::new(
                floats::<3>(&b["origin"]),
                floats::<3>(&b["run"]),
                floats::<3>(&b["u"]),
                floats::<3>(&b["v"]),
            )
            .unwrap();
            let got = prove_envelope(&ctx, mouth, &walls, &base)
                .map_or(Value::Null, |p| envelope_json(&p));
            if !common::same(&tied_terms(&got), &tied_terms(&call["proof"])) {
                tally.found.push((
                    json!({"case": "envelope_call", "file": file, "mouth": mouth, "walls": walls}),
                    format!(
                        "{file} envelope {mouth} {walls:?}: {}",
                        common::diff(&got, &call["proof"])
                    ),
                ));
            }
        }
        tally
    });
    let seat_calls: usize = tallies.iter().map(|t| t.seat_calls).sum();
    let envelope_calls: usize = tallies.iter().map(|t| t.envelope_calls).sum();
    let found: Vec<(Value, String)> = tallies.into_iter().flat_map(|t| t.found).collect();
    let mut problems = Vec::new();
    for case in ["seats", "envelopes", "seat_call", "envelope_call"] {
        let mine: Vec<_> = found
            .iter()
            .filter(|(identity, _)| identity["case"] == case)
            .cloned()
            .collect();
        eprintln!("{case}: {} differ", mine.len());
        if let Err(e) = std::panic::catch_unwind(|| check_known(case, mine, Some(&ran))) {
            problems.push(
                e.downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| "check failed".into()),
            );
        }
    }
    let nothing = runs
        .iter()
        .filter(|r| r["source"] == "test" && r["seats"] == json!([]) && r["envelopes"] == json!([]))
        .count();
    eprintln!(
        "{} parts ({nothing} test parts on which Python proves nothing, {} more left out by the \
         capture), {seat_calls} seat calls, {envelope_calls} envelope calls replayed",
        ran.len(),
        captured["unselected"]
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(
        (nothing, captured["unselected"].as_u64()),
        (NO_PROOF_TEST_PARTS, Some(UNSELECTED_TEST_PARTS)),
        "the capture's test parts on which Python proves nothing, and those it left out, changed: \
         update the counts after checking the recapture"
    );
}

/// Test parts on which Python proves nothing that the capture keeps (a fixed sample per test).
const NO_PROOF_TEST_PARTS: usize = 60;
/// Test parts on which Python proves nothing that the capture leaves out.
const UNSELECTED_TEST_PARTS: u64 = 428;

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

/// What a part proves independent of where it sits: per proof its faces and its placement-free
/// values (the axial length and arc bulge of a seat; the section, the interval's length, the
/// terms' gradients' size and the volume of an envelope passage), sorted.
fn placement_free(part: &Part) -> Value {
    let ctx = Context::new(part);
    let mut seats: Vec<Value> = cylindrical_seat_proofs(&ctx)
        .iter()
        .map(|p| {
            let chord = [0, 1].map(|i| p.boundary[1].point[i] - p.boundary[0].point[i]);
            json!({
                "walls": p.walls,
                "context": p.context,
                "length": p.run_interval.1 - p.run_interval.0,
                "chord": chord[0].hypot(chord[1]),
                "bulge": p.boundary[0].bulge.abs(),
            })
        })
        .collect();
    seats.sort_by_key(|s| indices(&s["walls"]));
    let mut envelopes: Vec<Value> = plane_envelope_passage_proofs(&ctx)
        .iter()
        .map(|p| {
            // Sorted: tied terms are ordered by round-off (see `tied_terms`).
            let mut slopes: Vec<f64> = p.terms.iter().map(|(_, g)| g[0].hypot(g[1])).collect();
            slopes.sort_by(f64::total_cmp);
            json!({
                "walls": p.walls,
                "planar_context": p.planar_context,
                "roof_contexts": p.roof_contexts,
                "area": p.section.area(),
                "sides": p.section.boundary().len(),
                "slopes": slopes,
                "volume": p.volume,
            })
        })
        .collect();
    envelopes.sort_by_key(|e| (e["planar_context"].as_u64(), indices(&e["walls"])));
    json!({"seats": seats, "envelopes": envelopes})
}

/// The runs on which Python proves something: those moved by
/// [`proofs_do_not_depend_on_placement`].
fn moved_runs(captured: &Value) -> Vec<&Value> {
    captured["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            ["seats", "envelopes"]
                .iter()
                .any(|k| r[k].as_array().is_some_and(|a| !a.is_empty()))
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
        let path = match source {
            "test" => dir().join(file),
            "fixture" => common::fixtures().join(file),
            _ => match common::corpus_dir() {
                Some(d) => d.join(file),
                None => return Vec::new(),
            },
        };
        let Some(part) = read(source, file) else {
            return Vec::new();
        };
        let want = placement_free(&part);
        let mut out = Vec::new();
        for (name, r, t) in MOTIONS {
            let placement: Placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]]);
            let moved =
                read_step_file_placed(&path, &placement).unwrap_or_else(|e| panic!("{file}: {e}"));
            let got = placement_free(&moved);
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
    check_known("invariance", found, Some(&ran));
}

sliced!(
    proofs_do_not_depend_on_placement,
    super::proofs_invariant,
    files: super::moved_files,
    [0 => slice_0, 1 => slice_1, 2 => slice_2]
);
