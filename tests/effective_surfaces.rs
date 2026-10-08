//! The effective-surface query against Python's answers (`tools/capture_effective_surfaces.py`,
//! `tests/fixtures/captured/effective_surfaces/faces.json.gz`): every face of the parts the
//! helper's and its consumers' Python tests build, the golden fixtures and the corpus, asked for
//! its effective fact, `recovery_nominal` / `recovery_tolerance`, and its surface use with a
//! material-side certificate.
//!
//! Compared: a refusal's reason (the port's `NotRecovered` stands for the three Python recovery
//! outcomes it does not tell apart); a fact's kind, provenance, parameters and requested
//! tolerance; the nominal and tolerance; a certificate's sign and probe distance, and a plane's
//! outward normal (a cylinder's `outward` is its first sample's radial direction, and the samples
//! are the port's own). Floats agree to one part in a million (`common::same`).
//!
//! Differences are listed in `tests/fixtures/captured/known_effective_surfaces.json`, each with
//! its `file`, `field` (`fact`, `nominal`, `use`, or `<motion>:<field>` for the invariance check)
//! and exactly the `faces` it covers, a verdict and a reason. Unlisted, stale and doubly listed
//! differences fail, and a failing run prints the entries it needs.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;

use quiddity::Part;
use quiddity::features::Context;
use quiddity::features::analytic_surfaces::{SurfaceKind, effective_fact};
use quiddity::features::effective_surfaces::{
    EffectiveFaces, SurfaceProvenance, SurfaceUse, cylinder_surface_dependency,
    physical_boundary_length, recovery_nominal, recovery_tolerance,
};
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

fn load_gz(name: &str) -> Value {
    let path = common::fixtures()
        .join("captured/effective_surfaces")
        .join(name);
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn kind_name(kind: SurfaceKind) -> &'static str {
    match kind {
        SurfaceKind::Plane => "plane",
        SurfaceKind::Cylinder => "cylinder",
        SurfaceKind::Cone => "cone",
        SurfaceKind::Sphere => "sphere",
    }
}

/// The port's answers for one face, shaped as the capture records Python's.
fn port_face(effective: &EffectiveFaces, part: &Part, face: usize) -> Value {
    let fact = match effective.fact(face) {
        Err(reason) => json!({"refused": reason.python_values()}),
        Ok(f) => json!({
            "kind": kind_name(f.kind()),
            "provenance": match f.provenance() {
                SurfaceProvenance::Native => "native",
                SurfaceProvenance::Recovered => "recovered",
            },
            "parameters": f.parameters(),
            "requested_tolerance": f.requested_tolerance,
        }),
    };
    let mut lengths = match (recovery_nominal(part, face), recovery_tolerance(part, face)) {
        (Some(n), Some(t)) => json!({"nominal": n, "tolerance": t}),
        _ => json!({"refused": true}),
    };
    // What the nominal is made of, for the message.
    lengths["area"] = json!(part.face_mass(face).map(|m| m[0]));
    lengths["perimeter"] = json!(physical_boundary_length(part, face));
    // Independent of Python: the face itself is oriented (a recovered primitive is not, but the
    // face's own surface and orientation still are), so its outward normal at each sample must
    // agree with the certified side.
    let orientation = match effective.surface_use(face, true) {
        Ok(SurfaceUse {
            material_side: Some(side),
            ..
        }) => json!(
            side.sample_points
                .iter()
                .zip(&side.outward_samples)
                .all(|(p, o)| part.outward_at(face, *p).is_some_and(|n| n
                    .iter()
                    .zip(o)
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
                    > 0.5))
        ),
        _ => Value::Null,
    };
    let surface_use = match effective.surface_use(face, true) {
        Err(r) => json!({"refused": r.reason.python_value()}),
        Ok(u) => {
            let side = u.material_side.as_ref().expect("asked for a material side");
            json!({
                "kind": kind_name(u.surface.kind()),
                "native": u.surface.oriented(),
                "sign": side.candidate_outward_sign,
                "outward": side.outward,
                "probe_distance": side.probe_distance,
            })
        }
    };
    json!({"fact": fact, "nominal": lengths, "use": surface_use, "orientation": orientation})
}

/// Whether the port's answer for one field agrees with Python's. Where the nominal lengths
/// differ (a listed `nominal` difference), what follows from them (a recovered fact's requested
/// tolerance, the probe distance) is not compared again.
fn agrees(field: &str, port: &Value, python: &Value, lengths_agree: bool) -> bool {
    match field {
        "fact" => match (&port["refused"], &python["refused"]) {
            (Value::Array(reasons), Value::String(r)) => reasons.iter().any(|x| x == r),
            (Value::Null, Value::Null) => {
                ["kind", "provenance", "parameters"]
                    .iter()
                    .all(|k| common::same(&port[k], &python[k]))
                    && (!lengths_agree
                        || common::same(
                            &port["requested_tolerance"],
                            &python["requested_tolerance"],
                        ))
            }
            _ => false,
        },
        "orientation" => port != false,
        "nominal" => match python.get("nominal_refused") {
            Some(_) => port["refused"] == true,
            None => {
                common::same(&port["nominal"], &python["nominal"])
                    && common::same(&port["tolerance"], &python["tolerance"])
            }
        },
        _ => match (&port["refused"], &python["refused"]) {
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Null, Value::Null) => {
                port["sign"] == python["sign"]
                    && (!lengths_agree
                        || common::same(&port["probe_distance"], &python["probe_distance"]))
                    && (port["kind"] != "plane"
                        || common::same(&port["outward"], &python["outward"]))
            }
            _ => false,
        },
    }
}

/// The Python answer for one field of a captured face.
fn python_field(face: &Value, field: &str) -> Value {
    match field {
        "nominal" => {
            let mut v = json!({});
            for k in [
                "nominal",
                "tolerance",
                "nominal_refused",
                "area",
                "adaptive_area",
                "perimeter",
                "precise_perimeter",
            ] {
                if !face[k].is_null() {
                    v[k] = face[k].clone();
                }
            }
            v
        }
        _ => face[field].clone(),
    }
}

/// A difference: (file, field) and the face, with what each side said.
type Difference = ((String, String), usize, String);

/// Checks the differences against the known list (entries for the given fields only): every
/// difference listed exactly once, every listed face differing.
fn check_known(found: Vec<Difference>, fields: &dyn Fn(&str) -> bool, files: &BTreeSet<String>) {
    let known = common::load("captured/known_effective_surfaces.json");
    let entries: Vec<&Value> = known
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| fields(e["field"].as_str().unwrap()))
        .collect();
    common::check_verdicts(
        "known_effective_surfaces.json",
        &entries.iter().map(|e| (*e).clone()).collect::<Vec<_>>(),
    );
    let mut listed: BTreeMap<(String, String), BTreeSet<usize>> = BTreeMap::new();
    let mut problems = Vec::new();
    for e in &entries {
        let key = (
            e["file"].as_str().unwrap().to_string(),
            e["field"].as_str().unwrap().to_string(),
        );
        let faces = listed.entry(key.clone()).or_default();
        for f in e["faces"].as_array().unwrap() {
            let f = f.as_u64().unwrap() as usize;
            if !faces.insert(f) {
                problems.push(format!("{key:?} face {f} is listed twice"));
            }
        }
    }
    let mut unlisted: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    let mut seen: BTreeMap<(String, String), BTreeSet<usize>> = BTreeMap::new();
    for (key, face, message) in found {
        seen.entry(key.clone()).or_default().insert(face);
        if !listed.get(&key).is_some_and(|l| l.contains(&face)) {
            problems.push(format!("{} {} face {face}: {message}", key.0, key.1));
            unlisted.entry(key).or_default().push(face);
        }
    }
    for (key, faces) in &listed {
        if !files.contains(&key.0) {
            continue;
        }
        let stale: Vec<usize> = faces
            .iter()
            .filter(|f| !seen.get(key).is_some_and(|s| s.contains(f)))
            .copied()
            .collect();
        if !stale.is_empty() {
            problems.push(format!("{} {}: stale faces {stale:?}", key.0, key.1));
        }
    }
    if !unlisted.is_empty() {
        let needed: Vec<Value> = unlisted
            .iter()
            .map(|((file, field), faces)| {
                json!({"file": file, "field": field, "faces": faces,
                       "verdict": "undetermined", "reason": "TODO"})
            })
            .collect();
        problems.push(format!(
            "entries needed:\n{}",
            serde_json::to_string_pretty(&needed).unwrap()
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

struct Run<'v> {
    source: &'v str,
    file: &'v str,
    faces: &'v [Value],
    refused: Option<&'v str>,
}

fn runs(captured: &Value) -> Vec<Run<'_>> {
    captured["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| Run {
            source: r["source"].as_str().unwrap(),
            file: r["file"].as_str().unwrap(),
            // A part Python refused, or whose answering died, keeps the faces it answered.
            faces: r["faces"].as_array().map_or(&[][..], Vec::as_slice),
            refused: r["refused"].as_str(),
        })
        .collect()
}

fn path(run: &Run, corpus: &Option<PathBuf>) -> Option<PathBuf> {
    Some(match run.source {
        "test" => common::fixtures()
            .join("captured/effective_surfaces")
            .join(run.file),
        "fixture" => common::fixtures().join(run.file),
        _ => corpus.as_ref()?.join(run.file),
    })
}

fn corpus() -> Option<PathBuf> {
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
    corpus
}

const FIELDS: [&str; 4] = ["fact", "nominal", "use", "orientation"];

#[test]
fn effective_surfaces_agree_with_python() {
    let captured = load_gz("faces.json.gz");
    let runs = runs(&captured);
    let corpus = corpus();
    let files: BTreeSet<String> = runs
        .iter()
        .filter(|r| path(r, &corpus).is_some())
        .map(|r| r.file.to_string())
        .collect();
    let results = common::parallel::map(&runs, |run| {
        let Some(path) = path(run, &corpus) else {
            return (0, Vec::new());
        };
        let part = read_step_file(&path).unwrap_or_else(|e| panic!("{}: {e}", run.file));
        if run.refused.is_none() {
            assert_eq!(
                part.faces.len(),
                run.faces.len(),
                "{}: face count",
                run.file
            );
        }
        let ctx = Context::new(&part);
        let effective = EffectiveFaces::new(&ctx);
        let mut out = Vec::new();
        for captured in run.faces {
            let face = captured["face"].as_u64().unwrap() as usize;
            let port = port_face(&effective, &part, face);
            // The fact is the one the ported families already read.
            assert_eq!(
                effective.fact(face).as_ref().ok().map(|f| &f.analytic),
                effective_fact(&part, face).as_ref(),
                "{} face {face}",
                run.file
            );
            // `cylinder_surface_dependency` certifies exactly the recovered faces.
            let dependency = cylinder_surface_dependency(&effective, face);
            let recovered = effective
                .fact(face)
                .as_ref()
                .is_ok_and(|f| f.provenance() == SurfaceProvenance::Recovered);
            assert_eq!(
                dependency.as_ref().map(|u| u.material_side.is_some()),
                effective
                    .surface_use(face, recovered)
                    .as_ref()
                    .map(|u| u.material_side.is_some()),
            );
            let lengths_agree = agrees(
                "nominal",
                &port["nominal"],
                &python_field(captured, "nominal"),
                true,
            );
            for field in FIELDS {
                let python = python_field(captured, field);
                // Not captured: OpenCascade did not finish meshing the face in time.
                if python["timeout"] == true {
                    continue;
                }
                if !agrees(field, &port[field], &python, lengths_agree) {
                    out.push((
                        (run.file.to_string(), field.to_string()),
                        face,
                        format!("port {}, Python {python}", port[field]),
                    ));
                }
            }
        }
        (run.faces.len(), out)
    });
    // The orientation check is the port's own, not a captured answer.
    let calls: usize = results.iter().map(|r| r.0).sum::<usize>() * (FIELDS.len() - 1);
    let timed_out = runs
        .iter()
        .flat_map(|r| r.faces)
        .filter(|f| f["use"]["timeout"] == true)
        .count();
    let refused: Vec<String> = runs
        .iter()
        .filter_map(|r| r.refused.map(|e| format!("{}: {e}", r.file)))
        .collect();
    eprintln!("not captured: {timed_out} uses timed out; parts refused: {refused:?}");
    let found: Vec<Difference> = results.into_iter().flat_map(|r| r.1).collect();
    eprintln!(
        "effective surfaces: {calls} captured answers, {} differ",
        found.len()
    );
    check_known(found, &|f| FIELDS.contains(&f), &files);
}

const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// A named motion: rotation rows and translation.
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);

/// Two of `tests/invariance.rs`'s rigid motions: one moving every axis and the origin, and one
/// reversing two axes.
const MOTIONS: [Motion; 2] = [
    (
        "rot_zx_moved",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [123.456, -78.9, 41.3],
    ),
    (
        "rot_y180",
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0; 3],
    ),
];

/// What does not depend on placement: the fact's kind, provenance, radius or semi-angle's size, or
/// its refusal; the nominal; the certificate's probe distance and outward side, or its refusal.
/// The canonical candidate normal (dominant component non-negative) turns with the axes, and so
/// does a cone's signed semi-angle: a plane's side is its outward normal turned back by the
/// motion's rotation *r*, a cylinder's the sign (whether its outward normal points away from the
/// axis).
fn placement_free(
    effective: &EffectiveFaces,
    part: &Part,
    face: usize,
    r: &[[f64; 3]; 3],
) -> [Value; 3] {
    let fact = match effective.fact(face) {
        Err(reason) => json!({"refused": reason.python_values()}),
        Ok(f) => json!({
            "kind": kind_name(f.kind()),
            "native": f.oriented(),
            "size": match f.kind() {
                SurfaceKind::Plane => Value::Null,
                SurfaceKind::Cylinder | SurfaceKind::Cone => json!(f.parameters()[6].abs()),
                SurfaceKind::Sphere => json!(f.parameters()[3]),
            },
        }),
    };
    let nominal = json!(recovery_nominal(part, face));
    let surface_use = match effective.surface_use(face, true) {
        Err(r) => json!(r.reason.python_value()),
        Ok(u) => {
            let side = u.material_side.as_ref().unwrap();
            let o = side.outward;
            let outward = match u.surface.kind() {
                SurfaceKind::Plane => {
                    json!([0, 1, 2].map(|j| (0..3).map(|i| r[i][j] * o[i]).sum::<f64>()))
                }
                _ => json!(side.candidate_outward_sign),
            };
            json!([outward, side.probe_distance])
        }
    };
    [fact, nominal, surface_use]
}

#[test]
fn effective_surfaces_are_placement_independent() {
    let Some(dir) = corpus() else { return };
    let names: Vec<String> = common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap().to_string())
        .collect();
    let results = common::parallel::map(&names, |name| {
        let path = dir.join(name);
        let base = read_step_file(&path).unwrap();
        let ctx = Context::new(&base);
        let effective = EffectiveFaces::new(&ctx);
        let want: Vec<[Value; 3]> = (0..base.faces.len())
            .map(|f| placement_free(&effective, &base, f, &IDENTITY))
            .collect();
        let mut out = Vec::new();
        for (motion, r, t) in MOTIONS {
            let placement: Placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]]);
            let moved = read_step_file_placed(&path, &placement).unwrap();
            let ctx = Context::new(&moved);
            let effective = EffectiveFaces::new(&ctx);
            for (face, want) in want.iter().enumerate() {
                let got = placement_free(&effective, &moved, face, &r);
                for (i, field) in FIELDS[..3].iter().enumerate() {
                    if !common::same(&got[i], &want[i]) {
                        out.push((
                            (name.clone(), format!("{motion}:{field}")),
                            face,
                            format!("moved {}, unmoved {}", got[i], want[i]),
                        ));
                    }
                }
            }
        }
        out
    });
    let found: Vec<Difference> = results.into_iter().flatten().collect();
    let files: BTreeSet<String> = names.into_iter().collect();
    eprintln!("effective surfaces under motion: {} differ", found.len());
    check_known(found, &|f| f.contains(':'), &files);
}
