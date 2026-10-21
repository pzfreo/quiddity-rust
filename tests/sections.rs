//! Rust snapshots of section values, patch questions and ring proposals. Proposal tie ordering and numeric tolerance are unchanged.

use super::common;
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

#[test]
fn rust_snapshot_section_values() {
    let inputs = load_gz("values.json.gz");
    let actual: Vec<Value> = inputs["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| value_answer(record["kind"].as_str().unwrap(), &record["given"]))
        .collect();
    assert!(!actual.is_empty());
    common::snapshot::check(&common::fixtures(), "section_values", &json!(actual));
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
fn rust_snapshot_section_patches() {
    let inputs = load_gz("patches.json.gz");
    let actual: Vec<bool> = inputs["patches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| {
            let patch = polygon(&record["patch"]);
            let supports: Vec<Part> = record["supports"]
                .as_array()
                .unwrap()
                .iter()
                .map(polygon)
                .collect();
            let refs: Vec<(&Part, usize)> = supports.iter().map(|s| (s, 0)).collect();
            covered_patch((&patch, 0), &refs)
        })
        .collect();
    assert!(!actual.is_empty());
    common::snapshot::check(&common::fixtures(), "section_patches", &json!(actual));
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
fn rust_snapshot_section_ring_proposals() {
    let inputs = load_gz("proposals.json.gz");
    let corpus = common::corpus_dir();
    assert!(corpus.is_some() || std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
    common::parallel::map(inputs["runs"].as_array().unwrap(), |run| {
        let file = run["file"].as_str().unwrap();
        let source = run["source"].as_str().unwrap();
        let path = match source {
            "test" => common::fixtures().join("captured/sections").join(file),
            "fixture" => common::fixtures().join(file),
            _ => {
                let Some(dir) = &corpus else { return };
                dir.join(file)
            }
        };
        let part = read_step_file(&path).unwrap();
        let ctx = Context::new(&part);
        let proposals = match section_ring_proposals(&ctx) {
            Ok(found) => json!({"proposals":found.iter().map(proposal_json).collect::<Vec<_>>()}),
            Err(e) => json!({"refused":e.0}),
        };
        let treatments: Vec<Value> = run["entry_treatments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| treatment_answer(&ctx, c))
            .collect();
        let equal = |a: &Value, b: &Value| {
            common::same(&a["treatments"], &b["treatments"])
                && match (a["rings"].get("proposals"), b["rings"].get("proposals")) {
                    (Some(a), Some(b)) => same_proposals(a, b),
                    _ => common::same(&a["rings"], &b["rings"]),
                }
        };
        common::snapshot::check_with(
            &common::fixtures(),
            &format!("sections/{source}/{file}"),
            &json!({"rings":proposals,"treatments":treatments}),
            &equal,
        );
    });
}
