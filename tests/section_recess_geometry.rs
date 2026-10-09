//! Section-recess candidates and their geometry against Python's
//! (`tools/capture_section_recess_geometry.py`, fixtures in
//! `tests/fixtures/captured/section_recess_geometry/`): `_section_recess_geometry` has no
//! `recognise_*` entry point, so its calls are replayed directly.
//!
//! `calls.json.gz` holds, per part (the parts the section-recess Python tests hand
//! `_candidates` or `has_physical_planar_floor`, the golden fixtures and the corpus):
//! `_candidates`; the three floor readers asked of every planar face on its own (only the
//! answers that are not `None` are listed); each seat, cylindrical-pocket, cylindrical-passage
//! and plane-envelope proof Python proves with what its projection makes of it (each proof is
//! rebuilt here from the captured values, so the projection is checked apart from the prover,
//! which `tests/section_recess_helpers.rs` and `tests/section_geometry.rs` check); and every
//! `has_physical_planar_floor` question with its answer. `channels.json.gz` holds
//! `cylindrical_channel_geometry` of every channel proof `tests/section_geometry.rs` replays.
//!
//! Floats agree to one part in a million (`common::same`); a refusal agrees by its message.
//! Differences are listed in `known_differences.json` beside the fixtures: per entry its `case`
//! (`candidates`, `floor`, `seat`, `pocket`, `passage`, `envelope`, `floor_question`, `channel`,
//! `invariance`), what identifies it (a part's `file`; a floor reading's `floor` and `kind`; a
//! seat's, passage's or envelope's `walls`; a pocket's `floor` and `stock`; a question's `walls`,
//! `axis` and `open_sign`; a channel's `defining`; an invariance entry's `motion`), a verdict
//! and a reason. A failing run prints the entries it needs.
//!
//! The candidates are also found under `tests/invariance.rs`'s rigid motions on every part
//! where Python finds one: the moved part must give the same candidates on the same faces, each
//! geometry the unmoved one carried by the motion and re-expressed in the canonical frame of its
//! moved run (its profile re-canonicalised, its interval, ends and end surfaces turned with the
//! run), within the two projections' displacement bounds (0.002 each).

mod common;
#[macro_use]
#[path = "support/slices.rs"]
mod slices;

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::io::Read;

use quiddity::Part;
use quiddity::features::Context;
use quiddity::features::cylindrical_channels::CylindricalChannelProof;
use quiddity::features::cylindrical_passages::CylindricalPassageProof;
use quiddity::features::cylindrical_pockets::CylindricalPocketProof;
use quiddity::features::cylindrical_seats::CylindricalSeatProof;
use quiddity::features::effective_surfaces::EffectiveFaces;
use quiddity::features::passages::PassageSectionVertex;
use quiddity::features::plane_envelope_passages::PlaneEnvelopePassageProof;
use quiddity::features::section_recess::{SectionRecessError, open_profile_material_side};
use quiddity::features::section_recess_discovery::discover_section_recesses;
use quiddity::features::section_recess_geometry::{
    Candidate, candidates, cylindrical_candidate, cylindrical_channel_geometry,
    cylindrical_passage_geometry, floor_readings, has_physical_planar_floor,
    plane_envelope_geometry, seat_geometry,
};
use quiddity::features::sections::{LocalFrame, PlanarSection, SectionVertex};
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const KNOWN: &str = "captured/section_recess_geometry/known_differences.json";

fn dir() -> std::path::PathBuf {
    common::fixtures().join("captured/section_recess_geometry")
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

fn index(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}

fn frame(v: &Value) -> LocalFrame {
    LocalFrame::new(
        floats::<3>(&v["origin"]),
        floats::<3>(&v["run"]),
        floats::<3>(&v["u"]),
        floats::<3>(&v["v"]),
    )
    .unwrap()
}

fn vertices(v: &Value) -> Vec<SectionVertex> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let [x, y, bulge] = floats::<3>(p);
            SectionVertex::new([x, y], bulge).unwrap()
        })
        .collect()
}

fn interval(v: &Value) -> (f64, f64) {
    let [a, b] = floats::<2>(v);
    (a, b)
}

fn seat_proof(p: &Value) -> CylindricalSeatProof {
    let boundary = vertices(&p["boundary"]);
    CylindricalSeatProof {
        walls: indices(&p["walls"]),
        context: indices(&p["context"]),
        owner: index(&p["owner"]),
        frame: frame(&p["frame"]),
        boundary: [boundary[0], boundary[1]],
        run_interval: interval(&p["run_interval"]),
    }
}

fn pocket_proof(p: &Value) -> CylindricalPocketProof {
    CylindricalPocketProof {
        floor: index(&p["floor"]),
        walls: indices(&p["walls"]),
        stock: index(&p["stock"]),
        owner: index(&p["owner"]),
        axis_point: floats(&p["axis_point"]),
        axis_direction: floats(&p["axis_direction"]),
        run: floats(&p["run"]),
        radius: p["radius"].as_f64().unwrap(),
        volume: p["volume"].as_f64().unwrap(),
    }
}

fn passage_proof(p: &Value) -> CylindricalPassageProof {
    CylindricalPassageProof {
        walls: indices(&p["walls"]),
        cylinder: index(&p["cylinder"]),
        planar_context: index(&p["planar_context"]),
        owner: index(&p["owner"]),
        frame: frame(&p["frame"]),
        section: PlanarSection::new(vertices(&p["section"])).unwrap(),
        run_interval: interval(&p["run_interval"]),
        cylindrical_end: index(&p["cylindrical_end"]),
        axis_point: floats(&p["axis_point"]),
        axis_direction: floats(&p["axis_direction"]),
        radius: p["radius"].as_f64().unwrap(),
        volume: p["volume"].as_f64().unwrap(),
    }
}

fn envelope_proof(p: &Value) -> PlaneEnvelopePassageProof {
    let term = |t: &Value| (t[0].as_f64().unwrap(), floats::<2>(&t[1]));
    let terms = p["terms"].as_array().unwrap();
    let roofs = indices(&p["roof_contexts"]);
    PlaneEnvelopePassageProof {
        walls: indices(&p["walls"]),
        planar_context: index(&p["planar_context"]),
        roof_contexts: [roofs[0], roofs[1]],
        owner: index(&p["owner"]),
        frame: frame(&p["frame"]),
        section: PlanarSection::new(vertices(&p["section"])).unwrap(),
        run_interval: interval(&p["run_interval"]),
        envelope_end: index(&p["envelope_end"]),
        terms: [term(&terms[0]), term(&terms[1])],
        volume: p["volume"].as_f64().unwrap(),
    }
}

fn channel_proof(p: &Value) -> CylindricalChannelProof {
    let bounds = p["bounds"].as_array().unwrap();
    CylindricalChannelProof {
        supports: indices(&p["supports"]),
        cylinder: index(&p["cylinder"]),
        planar_context: index(&p["planar_context"]),
        owner: index(&p["owner"]),
        run_axis: p["run_axis"].as_str().unwrap().to_owned(),
        width_axis: p["width_axis"].as_str().unwrap().to_owned(),
        open_sign: p["open_sign"].as_i64().unwrap() as i32,
        bounds: std::array::from_fn(|i| interval(&bounds[i])),
        run_interval: interval(&p["run_interval"]),
        cylindrical_end: index(&p["cylindrical_end"]),
        axis_point: floats(&p["axis_point"]),
        axis_direction: floats(&p["axis_direction"]),
        radius: p["radius"].as_f64().unwrap(),
        volume: p["volume"].as_f64().unwrap(),
    }
}

fn candidate_json(c: &Candidate) -> Value {
    json!({
        "defining": c.defining_faces,
        "constituent": c.constituent_faces,
        "mouth": c.mouth,
        "body": c.body,
        "section_shape": c.section_shape,
        "feature_kind": c.feature_kind,
        "geometry": serde_json::to_value(&c.geometry).unwrap(),
    })
}

/// A refusal as Python names it: its exception class and message.
fn raised(e: &SectionRecessError) -> String {
    let class = if e.0 == "float division by zero" {
        "ZeroDivisionError"
    } else {
        "ValueError"
    };
    format!("{class}: {}", e.0)
}

/// The port's answer in the captured call's shape: *key* set to the value, or `raised`.
fn answer<T>(key: &str, got: Result<T, SectionRecessError>, shape: impl Fn(&T) -> Value) -> Value {
    match got {
        Ok(v) => json!({ key: shape(&v) }),
        Err(e) => json!({ "raised": raised(&e) }),
    }
}

/// The captured call's answer: its value or its refusal, without its inputs.
fn captured_answer(call: &Value, key: &str) -> Value {
    match call.get("raised") {
        Some(r) => json!({ "raised": r }),
        None => json!({ key: call[key] }),
    }
}

/// Checks *found* differences (identity, description) against the listed entries of *case*, and
/// fails on an unlisted difference or a listed entry that no longer differs (of the files *ran*:
/// without the corpus its entries are not checked).
fn check_known(case: &str, found: Vec<(Value, String)>, ran: &BTreeSet<String>) {
    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts("section_recess_geometry/known_differences.json", known);
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

fn runs_ran(runs: &[&Value]) -> BTreeSet<String> {
    runs.iter()
        .filter(|r| common::corpus_dir().is_some() || r["source"] != "corpus")
        .map(|r| r["file"].as_str().unwrap().to_string())
        .collect()
}

fn require_corpus() {
    if common::corpus_dir().is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
}

/// The candidates as the capture lists them, or the refusal.
fn candidates_json(ctx: &Context<'_>, surfaces: &EffectiveFaces<'_, '_>) -> Value {
    match candidates(ctx, surfaces) {
        Ok(found) => json!(found.iter().map(candidate_json).collect::<Vec<_>>()),
        Err(e) => json!({ "refused": raised(&e) }),
    }
}

const CASES: [&str; 7] = [
    "candidates",
    "floor",
    "seat",
    "pocket",
    "passage",
    "envelope",
    "floor_question",
];

/// What one part's replay found.
#[derive(Default)]
struct Tally {
    calls: [usize; 7],
    /// Floor questions the capture could not answer (a timed-out or refused projection).
    uncaptured: usize,
    found: Vec<(Value, String)>,
}

impl Tally {
    fn compare(&mut self, case: &str, identity: Value, got: &Value, want: &Value) {
        self.calls[CASES.iter().position(|&c| c == case).unwrap()] += 1;
        if !common::same(got, want) {
            let text = format!("{case} {identity}: {}", common::diff(got, want));
            self.found.push((identity, text));
        }
    }
}

fn replay(run: &Value, part: &Part) -> Tally {
    let file = run["file"].as_str().unwrap();
    let id = |case: &str, more: Value| {
        let mut identity = json!({"case": case, "file": file});
        for (k, v) in more.as_object().unwrap() {
            identity[k] = v.clone();
        }
        identity
    };
    let mut tally = Tally::default();
    let ctx = Context::new(part);
    let surfaces = EffectiveFaces::new(&ctx);
    tally.compare(
        "candidates",
        id("candidates", json!({})),
        &candidates_json(&ctx, &surfaces),
        &run["candidates"],
    );
    // Discovery numbers the candidates as records: one per candidate, its faces as evidence
    // (Python's `discover_section_recesses` builds the same records from the same candidates).
    if let Some(want) = run["candidates"].as_array() {
        let found = discover_section_recesses(&ctx, &surfaces)
            .unwrap_or_else(|e| panic!("{file}: discovery refused: {}", e.0));
        let got: Vec<Value> = found
            .iter()
            .map(|o| {
                let mut all = o.defining.clone();
                all.extend(&o.context);
                all.sort_unstable();
                json!({"index": o.record.index(), "defining": o.defining, "constituent": all})
            })
            .collect();
        let expected: Vec<Value> = want
            .iter()
            .enumerate()
            .map(|(i, c)| json!({"index": i, "defining": c["defining"], "constituent": c["constituent"]}))
            .collect();
        if got != expected {
            // Only a part whose candidates differ (listed under `candidates`) may differ here.
            let listed = common::load(KNOWN)
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k["case"] == "candidates" && k["file"] == run["file"]);
            assert!(listed, "{file}: discovery {got:?}, candidates {expected:?}");
        }
    }
    // Every planar face's three readings: the listed answers, every other one `None`.
    let kinds = ["obround", "polygonal", "mixed"];
    let mut planar = 0;
    let mut readings = Vec::new();
    for floor in 0..part.faces.len() {
        if !quiddity::features::graph::is_planar(part, floor) {
            continue;
        }
        planar += 1;
        for (kind, reading) in kinds.iter().zip(floor_readings(&ctx, floor)) {
            match reading {
                Ok(None) => {}
                Ok(Some(c)) => readings
                    .push(json!({"floor": floor, "kind": kind, "candidate": candidate_json(&c)})),
                Err(e) => {
                    readings.push(json!({"floor": floor, "kind": kind, "raised": raised(&e)}))
                }
            }
        }
    }
    assert_eq!(
        planar,
        run["planar"].as_u64().unwrap() as usize,
        "{file}: planar faces"
    );
    let want = run["floors"].as_array().unwrap();
    let key = |r: &Value| (r["floor"].as_u64(), r["kind"].as_str().map(str::to_owned));
    let mut asked: BTreeSet<_> = readings.iter().map(key).collect();
    asked.extend(want.iter().map(key));
    for k in asked {
        let pick = |list: &[Value]| {
            list.iter()
                .find(|r| key(r) == k)
                .map_or(Value::Null, |r| r.clone())
        };
        let (got, wanted) = (pick(&readings), pick(want));
        tally.compare(
            "floor",
            id("floor", json!({"floor": k.0, "kind": k.1})),
            &got,
            &wanted,
        );
    }
    // Every planar face was asked all three readers.
    tally.calls[1] = 3 * planar;
    if let Some(calls) = run["seats"].as_array() {
        for call in calls {
            let proof = seat_proof(&call["proof"]);
            let got = answer("geometry", seat_geometry(&proof), |g| {
                serde_json::to_value(g).unwrap()
            });
            let identity = id("seat", json!({"walls": call["proof"]["walls"]}));
            tally.compare("seat", identity, &got, &captured_answer(call, "geometry"));
        }
    }
    if let Some(calls) = run["pockets"].as_array() {
        for call in calls {
            let proof = pocket_proof(&call["proof"]);
            let got = answer(
                "candidate",
                cylindrical_candidate(part, &proof),
                candidate_json,
            );
            let identity = id(
                "pocket",
                json!({"floor": call["proof"]["floor"], "stock": call["proof"]["stock"]}),
            );
            tally.compare(
                "pocket",
                identity,
                &got,
                &captured_answer(call, "candidate"),
            );
        }
    }
    if let Some(calls) = run["passages"].as_array() {
        for call in calls {
            let proof = passage_proof(&call["proof"]);
            let got = answer("geometry", cylindrical_passage_geometry(&proof), |g| {
                serde_json::to_value(g).unwrap()
            });
            let identity = id(
                "passage",
                json!({"walls": call["proof"]["walls"], "cylinder": call["proof"]["cylinder"]}),
            );
            tally.compare(
                "passage",
                identity,
                &got,
                &captured_answer(call, "geometry"),
            );
        }
    }
    if let Some(calls) = run["envelopes"].as_array() {
        for call in calls {
            let proof = envelope_proof(&call["proof"]);
            let got = answer("geometry", plane_envelope_geometry(&proof), |g| {
                serde_json::to_value(g).unwrap()
            });
            let identity = id("envelope", json!({"walls": call["proof"]["walls"]}));
            tally.compare(
                "envelope",
                identity,
                &got,
                &captured_answer(call, "geometry"),
            );
        }
    }
    match run["floor_questions"].as_array() {
        None => tally.uncaptured += 1,
        Some(calls) => {
            for call in calls {
                if call.get("refused").is_some() || call.get("raised").is_some() {
                    tally.uncaptured += 1;
                    continue;
                }
                let set = |v: &Value| indices(v).into_iter().collect::<BTreeSet<usize>>();
                let got = has_physical_planar_floor(
                    part,
                    &set(&call["walls"]),
                    &set(&call["constituent"]),
                    call["axis"].as_str().unwrap(),
                    call["open_sign"].as_i64().unwrap() as i32,
                    call["published_floor"].as_f64().unwrap(),
                );
                let identity = id(
                    "floor_question",
                    json!({"walls": call["walls"], "axis": call["axis"],
                           "open_sign": call["open_sign"]}),
                );
                tally.compare("floor_question", identity, &json!(got), &call["answer"]);
            }
        }
    }
    tally
}

#[test]
fn section_recess_geometry_agrees_with_python() {
    require_corpus();
    let captured = load_gz("calls.json.gz");
    let runs: Vec<&Value> = captured["runs"].as_array().unwrap().iter().collect();
    let ran = runs_ran(&runs);
    let tallies = common::parallel::map(&runs, |run| {
        let Some(part) = read(
            run["source"].as_str().unwrap(),
            run["file"].as_str().unwrap(),
        ) else {
            return Tally::default();
        };
        replay(run, &part)
    });
    let mut calls = [0; 7];
    let mut uncaptured = 0;
    let mut found = Vec::new();
    for t in tallies {
        for (total, n) in calls.iter_mut().zip(t.calls) {
            *total += n;
        }
        uncaptured += t.uncaptured;
        found.extend(t.found);
    }
    let mut problems = Vec::new();
    for case in CASES {
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
        "{} parts; calls replayed: {}; {uncaptured} floor-question runs or calls not captured",
        ran.len(),
        CASES
            .iter()
            .zip(calls)
            .map(|(c, n)| format!("{n} {c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn channel_geometry_agrees_with_python() {
    let captured = load_gz("channels.json.gz");
    let calls = captured["calls"].as_array().unwrap();
    assert!(!calls.is_empty());
    let found: Vec<(Value, String)> =
        calls
            .iter()
            .filter_map(|call| {
                let got = answer(
                    "geometry",
                    cylindrical_channel_geometry(&channel_proof(&call["proof"])),
                    |g| serde_json::to_value(g).unwrap(),
                );
                let want = captured_answer(call, "geometry");
                (!common::same(&got, &want)).then(|| {
                (
                    json!({"case": "channel", "file": call["file"], "defining": call["defining"]}),
                    format!("channel {}: {}", call["defining"], common::diff(&got, &want)),
                )
            })
            })
            .collect();
    eprintln!("{} channel calls, {} differ", calls.len(), found.len());
    let ran = calls
        .iter()
        .map(|c| c["file"].as_str().unwrap().to_owned())
        .collect();
    check_known("channel", found, &ran);
}

// --- rigid motions ----------------------------------------------------------------------------

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

/// Two projections of one geometry each move it by at most 0.002.
const PLACEMENT_TOL: f64 = 0.004;

type V3 = [f64; 3];
type V2 = [f64; 2];

fn rotate(r: &[[f64; 3]; 3], v: V3) -> V3 {
    [0, 1, 2].map(|i| r[i][0] * v[0] + r[i][1] * v[1] + r[i][2] * v[2])
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn round(value: f64, digits: i32) -> f64 {
    let scale = 10f64.powi(digits);
    (value * scale).round() / scale
}

fn point(v: &Value) -> V2 {
    floats::<2>(v)
}

fn vertex_list(boundary: &Value) -> Vec<(V2, f64)> {
    boundary
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (point(&v["point"]), v["bulge"].as_f64().unwrap()))
        .collect()
}

fn vertex_json(list: &[(V2, f64)]) -> Value {
    json!(
        list.iter()
            .map(|(p, b)| json!({"point": p, "bulge": b}))
            .collect::<Vec<_>>()
    )
}

/// One end surface carried into the moved frame: *m* maps the old section coordinates to the
/// new, a run coordinate t becomes `s t + k`.
fn moved_surface(surface: &Value, m: [[f64; 2]; 2], s: f64, k: f64) -> Value {
    let map = |p: V2| {
        [
            m[0][0] * p[0] + m[0][1] * p[1],
            m[1][0] * p[0] + m[1][1] * p[1],
        ]
    };
    let gradient = |g: V2| map(g).map(|c| s * c);
    match surface["type"].as_str().unwrap() {
        "plane" => json!({"type": "plane", "gradient": gradient(point(&surface["gradient"]))}),
        "plane_envelope" => {
            let mut terms: Vec<(f64, V2)> = surface["terms"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    (
                        s * t["height"].as_f64().unwrap() + k,
                        gradient(point(&t["gradient"])),
                    )
                })
                .collect();
            terms.sort_by(|a, b| {
                (a.0, a.1[0], a.1[1])
                    .partial_cmp(&(b.0, b.1[0], b.1[1]))
                    .unwrap_or(Ordering::Equal)
            });
            let operator = match (surface["operator"].as_str().unwrap(), s > 0.0) {
                (op, true) => op.to_owned(),
                ("min", false) => "max".to_owned(),
                _ => "min".to_owned(),
            };
            json!({"type": "plane_envelope", "operator": operator,
                   "terms": terms.iter().map(|(h, g)| json!({"height": h, "gradient": g}))
                       .collect::<Vec<_>>()})
        }
        "cylinder" => {
            let p = floats::<3>(&surface["axis_point"]);
            let xy = map([p[0], p[1]]);
            let mut d = map(point(&surface["axis_direction"]));
            let dominant = if d[0].abs() > d[1].abs() { 0 } else { 1 };
            if d[dominant] < 0.0 {
                d = [-d[0], -d[1]];
            }
            let branch = match (surface["branch"].as_str().unwrap(), s > 0.0) {
                (b, true) => b.to_owned(),
                ("positive", false) => "negative".to_owned(),
                _ => "positive".to_owned(),
            };
            json!({"type": "cylinder", "axis_point": [xy[0], xy[1], s * p[2] + k],
                   "axis_direction": d, "radius": surface["radius"], "branch": branch})
        }
        other => panic!("unknown end surface {other}"),
    }
}

/// The geometry carried by the motion and re-expressed in its moved run's canonical frame.
fn moved_geometry(g: &Value, r: &[[f64; 3]; 3], t: V3) -> Value {
    let f = &g["frame"];
    let o = rotate(r, floats::<3>(&f["origin"]));
    let o = [0, 1, 2].map(|i| o[i] + t[i]);
    let (d, u, v) = (
        rotate(r, floats::<3>(&f["run"])),
        rotate(r, floats::<3>(&f["u"])),
        rotate(r, floats::<3>(&f["v"])),
    );
    let c = LocalFrame::canonical(d, o).unwrap();
    let s = dot(d, c.run).signum();
    let k = dot(t, c.run);
    let m = [[dot(u, c.u), dot(v, c.u)], [dot(u, c.v), dot(v, c.v)]];
    let map = |p: V2| {
        [
            m[0][0] * p[0] + m[0][1] * p[1],
            m[1][0] * p[0] + m[1][1] * p[1],
        ]
    };
    let [lo, hi] = floats::<2>(&g["run_interval"]);
    let run_interval = if s > 0.0 {
        [lo + k, hi + k]
    } else {
        [-hi + k, -lo + k]
    };
    let ends = &g["ends"];
    let end = |e: &Value| {
        json!({"condition": e["condition"],
               "surface": moved_surface(&e["surface"], m, s, k)})
    };
    let (low, high) = if s > 0.0 {
        (end(&ends["low"]), end(&ends["high"]))
    } else {
        (end(&ends["high"]), end(&ends["low"]))
    };
    let profile = &g["profile"];
    let mapped: Vec<(V2, f64)> = vertex_list(&profile["boundary"])
        .into_iter()
        .map(|(p, b)| (map(p), s * b))
        .collect();
    let profile = if profile["closure"] == "closed" {
        let section = PlanarSection::new(
            mapped
                .iter()
                .map(|&(p, b)| SectionVertex::new(p, b).unwrap())
                .collect(),
        )
        .unwrap();
        let list: Vec<(V2, f64)> = section
            .boundary()
            .iter()
            .map(|v| (v.point, v.bulge))
            .collect();
        json!({"closure": "closed", "boundary": vertex_json(&list)})
    } else {
        let n = mapped.len();
        let reversed: Vec<(V2, f64)> = (0..n)
            .map(|i| {
                let bulge = if i < n - 1 { -mapped[n - 2 - i].1 } else { 0.0 };
                (mapped[n - 1 - i].0, bulge)
            })
            .collect();
        let serial = |l: &[(V2, f64)]| -> Vec<f64> {
            l.iter()
                .flat_map(|(p, b)| [round(p[0], 4), round(p[1], 4), *b])
                .collect()
        };
        let chain = if serial(&reversed).partial_cmp(&serial(&mapped)) == Some(Ordering::Less) {
            reversed
        } else {
            mapped
        };
        let vertices: Vec<PassageSectionVertex> = chain
            .iter()
            .map(|&(point, bulge)| PassageSectionVertex { point, bulge })
            .collect();
        json!({"closure": "open", "boundary": vertex_json(&chain),
               "opening": [chain[chain.len() - 1].0, chain[0].0],
               "material_side": open_profile_material_side(&vertices).unwrap()})
    };
    json!({"type": "section_recess",
           "frame": {"origin": c.origin, "run": c.run, "u": c.u, "v": c.v},
           "run_interval": run_interval, "profile": profile,
           "ends": {"low": low, "high": high}})
}

/// Structural equality with numbers within *tol*.
fn close(a: &Value, b: &Value, tol: f64) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= tol
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q, tol))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| close(v, w, tol)))
        }
        _ => a == b,
    }
}

/// A closed profile's boundary agrees up to its start vertex (the canonical start is the
/// least serialized vertex, which two placements may round to different sides of a tie).
fn same_geometry(got: &Value, want: &Value) -> bool {
    if close(got, want, PLACEMENT_TOL) {
        return true;
    }
    if got["profile"]["closure"] != "closed" || want["profile"]["closure"] != "closed" {
        return false;
    }
    let boundary = got["profile"]["boundary"].as_array().unwrap();
    (1..boundary.len()).any(|shift| {
        let mut turned = got.clone();
        let rotated: Vec<Value> = boundary[shift..]
            .iter()
            .chain(&boundary[..shift])
            .cloned()
            .collect();
        turned["profile"]["boundary"] = json!(rotated);
        close(&turned, want, PLACEMENT_TOL)
    })
}

/// The runs where Python finds a candidate: those moved by [`candidates_do_not_depend_on_placement`].
fn moved_runs(captured: &Value) -> Vec<&Value> {
    captured["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["candidates"].as_array().is_some_and(|a| !a.is_empty()))
        .collect()
}

/// The files of the moved runs, each once, in run order: what the invariance test is sliced by,
/// so every run of a file (and every listed entry of it) is in one slice.
fn moved_files() -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for run in moved_runs(&load_gz("calls.json.gz")) {
        let file = run["file"].as_str().unwrap();
        if !files.iter().any(|f| f == file) {
            files.push(file.to_string());
        }
    }
    files
}

/// The candidates under every motion: one test per slice of the moved runs' files
/// (`tests/support/slices.rs`).
fn candidates_invariant(k: usize, n: usize) {
    require_corpus();
    let captured = load_gz("calls.json.gz");
    let all = moved_runs(&captured);
    assert!(!all.is_empty());
    let mine = slices::slice(&moved_files(), k, n);
    let runs: Vec<&Value> = all
        .into_iter()
        .filter(|r| mine.iter().any(|f| r["file"] == f.as_str()))
        .collect();
    // Every run's part unmoved and under every motion, all spread over every core at once (so a
    // costly part's readings run side by side); the candidates come back in run order, motion by
    // motion. A part the corpus does not have reads as `None`.
    let readings: Vec<(usize, Option<&Motion>)> = (0..runs.len())
        .flat_map(|i| {
            std::iter::once(None)
                .chain(MOTIONS.iter().map(Some))
                .map(move |m| (i, m))
        })
        .collect();
    let mut read_all = common::parallel::map(&readings, |&(i, motion)| {
        let file = runs[i]["file"].as_str().unwrap();
        let source = runs[i]["source"].as_str().unwrap();
        let part = match motion {
            None => read(source, file)?,
            Some((_, r, t)) => {
                let placement: Placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]]);
                read_step_file_placed(&path(source, file)?, &placement)
                    .unwrap_or_else(|e| panic!("{file}: {e}"))
            }
        };
        let ctx = Context::new(&part);
        let surfaces = EffectiveFaces::new(&ctx);
        Some(candidates(&ctx, &surfaces))
    })
    .into_iter();
    let found: Vec<(Value, String)> = runs
        .iter()
        .flat_map(|run| {
            let file = run["file"].as_str().unwrap();
            let unmoved = read_all.next().unwrap();
            let moved: Vec<_> = read_all.by_ref().take(MOTIONS.len()).collect();
            let Some(want) = unmoved else {
                return Vec::new();
            };
            let want = want.unwrap();
            let mut out = Vec::new();
            for ((name, r, t), got) in MOTIONS.iter().zip(moved) {
                let got = got.unwrap();
                let problem = match got {
                    Err(e) => Some(format!("refused: {}", e.0)),
                    Ok(got) if got.len() != want.len() => Some(format!(
                        "{} candidates moved, {} unmoved: moved {:?}, unmoved {:?}",
                        got.len(),
                        want.len(),
                        got.iter().map(|c| &c.constituent_faces).collect::<Vec<_>>(),
                        want.iter()
                            .map(|c| &c.constituent_faces)
                            .collect::<Vec<_>>()
                    )),
                    Ok(got) => got.iter().zip(&want).find_map(|(g, w)| {
                        let mut gj = candidate_json(g);
                        let mut wj = candidate_json(w);
                        let expected = moved_geometry(&wj["geometry"], r, *t);
                        let geometry = gj["geometry"].take();
                        wj["geometry"] = Value::Null;
                        if gj != wj {
                            Some(format!("faces or class moved {gj}, unmoved {wj}"))
                        } else if !same_geometry(&geometry, &expected) {
                            Some(format!(
                                "{:?} geometry\n  moved    {geometry}\n  expected {expected}",
                                g.constituent_faces
                            ))
                        } else {
                            None
                        }
                    }),
                };
                if let Some(text) = problem {
                    out.push((
                        json!({"case": "invariance", "file": file, "motion": name}),
                        format!("{file} {name}: {text}"),
                    ));
                }
            }
            out
        })
        .collect();
    check_known("invariance", found, &runs_ran(&runs));
}

sliced!(
    candidates_do_not_depend_on_placement,
    super::candidates_invariant,
    files: super::moved_files,
    [0 => slice_0, 1 => slice_1, 2 => slice_2, 3 => slice_3]
);
