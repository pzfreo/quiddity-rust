//! The section helpers against Python's answers (`tools/capture_sections.py`, fixtures in
//! `tests/fixtures/captured/sections/`): `_sections`, `_support_patches`, `_entry_treatments` and
//! `_section_passages` have no `recognise_*` entry point, so their calls are replayed directly.
//!
//! - `values.json.gz`: every section, frame, end-separation and occurrence-projection value the
//!   Python tests of these modules build, with its answer or its refusal (the same message);
//! - `patches.json.gz`: every straight-edged polygon `covered_patch` question those tests ask;
//! - `proposals.json.gz`: `section_ring_proposals` on every part those tests hand it, the golden
//!   fixtures and the corpus, and every `prove_entry_treatments` question asked on the way,
//!   each replayed on its own.
//!
//! Floats agree to one part in a million (`common::same`), refusals exactly (a proposal run's
//! captured refusal without its `<ExceptionType>: ` prefix). A part's proposals are compared in
//! the order each side emits them, except within a run of adjacent proposals whose order
//! round-off alone decides: Python's order between those is not stable from one Python run to
//! the next (`same_proposals`), so such a run is compared as a set. A value whose input the
//! port's types cannot hold (a section mutated after construction) is reported as not
//! constructible rather than refused by this test. Differences are
//! listed in `tests/fixtures/captured/known_sections.json`: per entry its `case` (`value`,
//! `patch`, `proposals`, `entry_treatment`), what identifies it (a value's `kind` and `given`
//! input, a patch's `index`, a part's `file`, a treatment question's `file`, `opening` and
//! `inner_wire`), a verdict and a reason. A failing run prints the entries it needs.

mod common;

use std::collections::BTreeSet;
use std::io::Read;

use quiddity::Part;
use quiddity::features::Context;
use quiddity::features::entry_treatments::prove_entry_treatments;
use quiddity::features::section_passages::{SectionRingProposal, section_ring_proposals};
use quiddity::features::sections::{
    BodyRefIssuer, LocalFrame, PlanarSection, SectionEnds, SectionError, SectionOccurrence,
    SectionVertex, occurrence_geometry, validate_section_end_separation,
};
use quiddity::features::support_patches::{covered_patch, polygon_face};
use quiddity::kernel::step::read_step_file;
use serde_json::{Value, json};

fn load_gz(name: &str) -> Value {
    let path = common::fixtures().join("captured/sections").join(name);
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
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

fn vertices(v: &Value) -> Result<Vec<SectionVertex>, SectionError> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| {
            let [px, py, bulge] = floats::<3>(x);
            SectionVertex::new([px, py], bulge)
        })
        .collect()
}

fn vertex_json(vs: &[SectionVertex]) -> Value {
    json!(
        vs.iter()
            .map(|v| [v.point[0], v.point[1], v.bulge])
            .collect::<Vec<_>>()
    )
}

fn frame_json(f: &LocalFrame) -> Value {
    json!({"origin": f.origin, "run": f.run, "u": f.u, "v": f.v})
}

/// The port's answer to one captured value call, in the capture's shape.
fn value_answer(kind: &str, given: &Value) -> Value {
    let refused = |e: SectionError| json!({"refused": e.0});
    match kind {
        "section" => match vertices(&given["boundary"]).and_then(PlanarSection::new) {
            Ok(s) => json!({
                "boundary": vertex_json(s.boundary()),
                "area": s.area(),
                "centroid": s.centroid(),
            }),
            Err(e) => refused(e),
        },
        "canonical" | "principal" => {
            let centroid = floats::<3>(&given["centroid"]);
            let frame = if kind == "canonical" {
                LocalFrame::canonical(floats::<3>(&given["first"]), centroid)
            } else {
                LocalFrame::principal(given["first"].as_str().unwrap(), centroid)
            };
            match frame {
                Ok(f) => json!({"frame": frame_json(&f)}),
                Err(e) => refused(e),
            }
        }
        "separation" => {
            let checked = vertices(&given["vertices"]).and_then(|vs| {
                validate_section_end_separation(
                    &vs,
                    float(&given["span"]),
                    floats::<2>(&given["gradient"]),
                    given["closed"].as_bool().unwrap(),
                )
            });
            match checked {
                Ok(()) => json!({"accepted": true}),
                Err(e) => refused(e),
            }
        }
        "geometry" => {
            let f = &given["frame"];
            let mut issuer = BodyRefIssuer::default();
            let ends = given["ends"].as_array().unwrap();
            let geometry = (|| {
                let frame = LocalFrame::new(
                    floats::<3>(&f["origin"]),
                    floats::<3>(&f["run"]),
                    floats::<3>(&f["u"]),
                    floats::<3>(&f["v"]),
                )?;
                let stored = vertices(&given["boundary"])?;
                let section = PlanarSection::new(stored.clone())?;
                // Python revalidates a stored section by rebuilding it, and refuses one whose
                // boundary was mutated after construction. A `PlanarSection` cannot be mutated,
                // so the port has no such input: the call is reported, not refused here.
                if section.boundary() != stored.as_slice() {
                    return Ok(None);
                }
                let [lo, hi] = floats::<2>(&given["run_interval"]);
                let occurrence = SectionOccurrence::new(
                    issuer.issue(None)?,
                    frame,
                    (lo, hi),
                    section,
                    SectionEnds::new(ends[0].as_bool().unwrap(), ends[1].as_bool().unwrap())?,
                )?;
                occurrence_geometry(&occurrence, &issuer).map(Some)
            })();
            match geometry {
                Ok(Some(g)) => json!({"geometry": serde_json::to_value(g).unwrap()}),
                Ok(None) => json!({"not_constructible": "a mutated section"}),
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
    let known = common::load("captured/known_sections.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_sections.json", known);
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
fn section_values_agree_with_python() {
    let captured = load_gz("values.json.gz");
    let values = captured["values"].as_array().unwrap();
    assert!(!values.is_empty());
    let mut found = Vec::new();
    let mut refusals = 0;
    for record in values {
        let kind = record["kind"].as_str().unwrap();
        let got = value_answer(kind, &record["given"]);
        let want = answer_of(record);
        if !common::same(&got, &want) {
            found.push((
                json!({"case": "value", "kind": kind, "given": record["given"]}),
                format!("{kind} {}: {}", record["given"], common::diff(&got, &want)),
            ));
        } else if got.get("refused").is_some() {
            refusals += 1;
        }
    }
    eprintln!(
        "section values: {} captured, {refusals} refusals matched, {} differ, {} skipped by the \
         capture",
        values.len(),
        found.len(),
        captured["skipped"]
    );
    check_known("value", found, None);
}

/// A captured polygon as a face of its own.
fn polygon(corners: &Value) -> Part {
    let points: Vec<[f64; 3]> = corners
        .as_array()
        .unwrap()
        .iter()
        .map(floats::<3>)
        .collect();
    polygon_face(&points).expect("a captured polygon has area")
}

#[test]
fn covered_patches_agree_with_python() {
    let captured = load_gz("patches.json.gz");
    let patches = captured["patches"].as_array().unwrap();
    assert!(!patches.is_empty());
    let mut found = Vec::new();
    for (index, record) in patches.iter().enumerate() {
        let patch = polygon(&record["patch"]);
        let supports: Vec<Part> = record["supports"]
            .as_array()
            .unwrap()
            .iter()
            .map(polygon)
            .collect();
        let refs: Vec<(&Part, usize)> = supports.iter().map(|s| (s, 0)).collect();
        let got = covered_patch((&patch, 0), &refs);
        if Some(got) != record["covered"].as_bool() {
            found.push((
                json!({"case": "patch", "index": index}),
                format!("patch {index}: port {got}, Python {}", record["covered"]),
            ));
        }
    }
    check_known("patch", found, None);
}

fn proposal_json(p: &SectionRingProposal) -> Value {
    let o = &p.occurrence;
    let ends = o.ends();
    let (lo, hi) = o.run_interval();
    json!({
        "frame": frame_json(o.frame()),
        "run_interval": [lo, hi],
        "boundary": vertex_json(o.section().boundary()),
        "ends": [ends.low_capped, ends.high_capped],
        "nodes": p.nodes,
        "solid": p.solid,
        "constituent": p.constituent,
        "low_gradient": p.low_gradient,
        "high_gradient": p.high_gradient,
    })
}

/// The proposal's sort key: run, interval, origin.
fn sort_key(p: &Value) -> Value {
    json!([p["frame"]["run"], p["run_interval"], p["frame"]["origin"]])
}

/// Whether round-off alone decides the order of two proposals: their sort keys are equal, or
/// the first component in which they differ agrees to one part in a million (`common::same`).
fn ordered_by_round_off(a: &Value, b: &Value) -> bool {
    let flat = |k: Value| -> Vec<f64> {
        k.as_array()
            .unwrap()
            .iter()
            .flat_map(|part| {
                part.as_array()
                    .unwrap()
                    .iter()
                    .map(float)
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let (x, y) = (flat(sort_key(a)), flat(sort_key(b)));
    x.iter()
        .zip(&y)
        .find(|(x, y)| x != y)
        .is_none_or(|(x, y)| common::same(&json!(x), &json!(y)))
}

/// Whether two proposal lists agree in order up to round-off: position by position in the
/// order each side emitted, except that a run of adjacent proposals whose order round-off alone
/// decides (`ordered_by_round_off`) is compared as a set against the same positions of the other
/// list. Python orders by exact keys, so last-bit round-off decides between two coincident
/// solids' rings, or between two congruent rings at one run interval (one ring's interval
/// `(-10.000000000000002, 10.0)`, the other's `(-10.0, 10.0)`, their origins opposite), and that
/// round-off is not even stable between Python runs: two captures ordered one part's coincident
/// rings both ways.
fn same_proposals(got: &Value, want: &Value) -> bool {
    let (Some(g), Some(w)) = (got.as_array(), want.as_array()) else {
        return common::same(got, want);
    };
    if g.len() != w.len() {
        return false;
    }
    let run_end = |list: &[Value], from: usize| {
        (from + 1..list.len())
            .find(|&i| !ordered_by_round_off(&list[i - 1], &list[i]))
            .unwrap_or(list.len())
    };
    let mut start = 0;
    while start < g.len() {
        let end = run_end(g, start).max(run_end(w, start));
        let mut unmatched: Vec<&Value> = g[start..end].iter().collect();
        for x in &w[start..end] {
            match unmatched.iter().position(|y| common::same(x, y)) {
                Some(i) => {
                    unmatched.remove(i);
                }
                None => return false,
            }
        }
        start = end;
    }
    true
}

/// A captured refusal without the `<ExceptionType>: ` prefix the capture writes before Python's
/// message (`ValueError: ...`), so it compares with the port's message alone.
fn python_message(refused: &str) -> &str {
    match refused.split_once(": ") {
        Some((name, message))
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') =>
        {
            message
        }
        _ => refused,
    }
}

#[test]
fn captured_refusals_lose_their_exception_type() {
    assert_eq!(
        python_message("ValueError: section boundary must be simple"),
        "section boundary must be simple"
    );
    assert_eq!(
        python_message("numpy.linalg.LinAlgError: Singular matrix: rank 2"),
        "Singular matrix: rank 2"
    );
    assert_eq!(
        python_message("section boundary must be simple"),
        "section boundary must be simple"
    );
    assert_eq!(
        python_message("run interval collapses: lo 1, hi 1"),
        "run interval collapses: lo 1, hi 1"
    );
}

#[test]
fn proposals_are_compared_in_order_except_within_ties() {
    let p = |run: f64, lo: f64, origin: f64| json!({"frame": {"run": [run], "origin": [origin]}, "run_interval": [lo]});
    let (a, b) = (p(0.0, 1.0, 0.0), p(1.0, 1.0, 5.0));
    // Order decided by distinct keys matters.
    assert!(same_proposals(&json!([a, b]), &json!([a, b])));
    assert!(!same_proposals(&json!([b, a]), &json!([a, b])));
    // Order decided by round-off does not, even where later components differ.
    let c = p(1.0, 1.0 + 1e-12, -5.0);
    assert!(same_proposals(&json!([a, c, b]), &json!([a, b, c])));
    assert!(!same_proposals(&json!([c, a, b]), &json!([a, b, c])));
}

/// The port's answer to one captured treatment question, in the capture's shape.
fn treatment_answer(ctx: &Context<'_>, call: &Value) -> Value {
    let part = ctx.part;
    let opening = call["opening"].as_u64().unwrap() as usize;
    let outer = part.outer_loop(opening);
    let inner: Vec<usize> = (0..part.faces[opening].loops.len())
        .filter(|&l| Some(l) != outer)
        .collect();
    let lp = &part.faces[opening].loops[inner[call["inner_wire"].as_u64().unwrap() as usize]];
    let mut wire: Vec<(usize, bool)> = Vec::new();
    for &(e, forward) in &lp.edges {
        if !wire.iter().any(|w| w.0 == e) {
            wire.push((e, forward));
        }
    }
    let seed: BTreeSet<usize> = call["seed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as usize)
        .collect();
    let proof = prove_entry_treatments(
        ctx,
        &seed,
        &wire,
        floats::<3>(&call["run"]),
        float(&call["at"]),
        float(&call["far"]),
    );
    match proof {
        None => Value::Null,
        Some(p) => json!({"treatments": p.treatments, "stock": p.stock}),
    }
}

#[test]
fn section_ring_proposals_agree_with_python() {
    let captured = load_gz("proposals.json.gz");
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
    let found = common::parallel::map(&runs, |run| {
        let file = run["file"].as_str().unwrap();
        let path = match run["source"].as_str().unwrap() {
            "test" => common::fixtures().join("captured/sections").join(file),
            "fixture" => common::fixtures().join(file),
            _ => match &corpus {
                Some(dir) => dir.join(file),
                None => return Vec::new(),
            },
        };
        let part = read_step_file(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
        let ctx = Context::new(&part);
        let mut out = Vec::new();
        let got = match section_ring_proposals(&ctx) {
            Ok(found) => json!({"proposals": found.iter().map(proposal_json).collect::<Vec<_>>()}),
            Err(e) => json!({"refused": e.0}),
        };
        let mut want = json!({});
        if !run["proposals"].is_null() {
            want["proposals"] = run["proposals"].clone();
        }
        if let Some(refused) = run["refused"].as_str() {
            want["refused"] = json!(python_message(refused));
        }
        let agree = match (got.get("proposals"), want.get("proposals")) {
            (Some(g), Some(w)) => same_proposals(g, w),
            _ => common::same(&got, &want),
        };
        if !agree {
            out.push((
                json!({"case": "proposals", "file": file}),
                format!("{file}: {}", common::diff(&got, &want)),
            ));
        }
        for call in run["entry_treatments"].as_array().unwrap() {
            let got = treatment_answer(&ctx, call);
            if !common::same(&got, &call["proof"]) {
                out.push((
                    json!({
                        "case": "entry_treatment",
                        "file": file,
                        "opening": call["opening"],
                        "inner_wire": call["inner_wire"],
                    }),
                    format!(
                        "{file} opening {}: port {got}, Python {}",
                        call["opening"], call["proof"]
                    ),
                ));
            }
        }
        out
    });
    let (proposals, treatments): (Vec<_>, Vec<_>) = found
        .into_iter()
        .flatten()
        .partition(|(identity, _)| identity["case"] == "proposals");
    let mut problems = Vec::new();
    for (case, found) in [("proposals", proposals), ("entry_treatment", treatments)] {
        if let Err(e) = std::panic::catch_unwind(|| check_known(case, found, Some(&ran))) {
            problems.push(
                e.downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| "check failed".into()),
            );
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
