//! Planar outer-profile evidence against Python's answers (`tools/capture_outer_profiles.py`,
//! `tests/fixtures/captured/outer_profiles/faces.json.gz`): every face of the parts quiddity's
//! outer-profile tests build, the golden fixtures draftwright's profile angles read and the
//! corpus, asked for `planar_outer_profile`.
//!
//! Compared per face: the outcome (a refusal's reason, or a profile), a profile's origin, normal,
//! inner-loop count, schema version and supports in order (kind, ends, and an arc's centre,
//! radius and sweep), and the faces of its body. Floats agree to one part in a million
//! (`common::same`). Independently of Python, every profile's supports must correspond to its
//! source edges: distinct edges of the face's outer loop, each with the support's vertices, kind
//! and circle.
//!
//! Differences are listed in `tests/fixtures/captured/outer_profiles/known_differences.json`,
//! each with its `file`, `field` (`outcome`, `profile`, `body`, or `<motion>:<field>` for the
//! invariance check) and exactly the `faces` it covers, a verdict and a reason. Unlisted, stale
//! and doubly listed differences fail, and a failing run prints the entries it needs.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;

use quiddity::Part;
use quiddity::features::outer_profile::{
    OuterProfileRefusalReason, PlanarOuterProfileEvidence, ProfileSupport, planar_outer_profile,
};
use quiddity::kernel::geom::Curve;
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const KNOWN: &str = "captured/outer_profiles/known_differences.json";

fn captured() -> Value {
    let path = common::fixtures().join("captured/outer_profiles/faces.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The port's answer for one face, shaped as the capture records Python's (the body as its
/// face roster rather than a position in the run's list).
fn port_face(part: &Part, face: usize) -> Value {
    match planar_outer_profile(part, face) {
        Err(r) => json!({"refused": r.reason.python_value()}),
        Ok(p) => {
            let v = serde_json::to_value(&p.profile).unwrap();
            json!({
                "origin": v["origin"],
                "normal": v["normal"],
                "inner_loop_count": v["inner_loop_count"],
                "schema_version": v["schema_version"],
                "supports": v["supports"],
                "body": p.body_faces,
            })
        }
    }
}

/// The fields of one face's answers that differ: (field, what each side said).
fn differences(port: &Value, python: &Value) -> Vec<(&'static str, String)> {
    let outcome = |v: &Value| v["refused"].as_str().unwrap_or("profile").to_string();
    if outcome(port) != outcome(python) {
        return vec![(
            "outcome",
            format!("port {}, Python {}", outcome(port), outcome(python)),
        )];
    }
    if !port["refused"].is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let keys = [
        "origin",
        "normal",
        "inner_loop_count",
        "schema_version",
        "supports",
    ];
    if !keys.iter().all(|k| common::same(&port[k], &python[k])) {
        let first = keys
            .iter()
            .find(|k| !common::same(&port[**k], &python[**k]))
            .unwrap();
        out.push((
            "profile",
            format!("{first}: port {}, Python {}", port[first], python[first]),
        ));
    }
    if port["body"] != python["body"] {
        out.push((
            "body",
            format!(
                "port {} faces, Python {}",
                port["body"].as_array().map_or(0, Vec::len),
                python["body"].as_array().map_or(0, Vec::len)
            ),
        ));
    }
    out
}

/// `assert_support_correspondence`: each support's source edge is a distinct edge of the face's
/// outer loop running between the support's ends, of the support's kind, an arc on its circle.
fn correspondence(part: &Part, p: &PlanarOuterProfileEvidence) -> Result<(), String> {
    let outer = part.outer_edges(p.face);
    if p.edges.len() != p.profile.supports.len() || p.edges.len() != outer.len() {
        return Err("support and edge counts differ".into());
    }
    let distinct: BTreeSet<usize> = p.edges.iter().copied().collect();
    if distinct.len() != p.edges.len() || !p.edges.iter().all(|e| outer.contains(e)) {
        return Err("edges are not the outer loop's, each once".into());
    }
    for (at, (support, &e)) in p.profile.supports.iter().zip(&p.edges).enumerate() {
        let edge = &part.edges[e];
        let ends = |a: [f64; 3], b: [f64; 3]| {
            let d = |p: [f64; 3], q: [f64; 3]| {
                (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt()
            };
            d(support.start(), a).max(d(support.end(), b)) < 1e-6
        };
        if !(ends(edge.start, edge.end) || ends(edge.end, edge.start)) {
            return Err(format!("support {at} does not run along edge {e}"));
        }
        let fits = match (support, &edge.curve) {
            (ProfileSupport::Line(_), Curve::Line { .. }) => true,
            (ProfileSupport::Arc(a), Curve::Circle { frame, radius }) => {
                (a.radius - radius).abs() < 1e-6
                    && (0..3).all(|i| (a.center[i] - frame.origin[i]).abs() < 1e-6)
            }
            _ => false,
        };
        if !fits {
            return Err(format!("support {at} is not edge {e}'s curve"));
        }
    }
    Ok(())
}

/// A difference: (file, field) and the face, with what each side said.
type Difference = ((String, String), usize, String);

/// Checks the differences against the known list (entries for the given fields only): every
/// difference listed exactly once, every listed face differing.
fn check_known(found: Vec<Difference>, fields: &dyn Fn(&str) -> bool, files: &BTreeSet<String>) {
    let known = common::load(KNOWN);
    let entries: Vec<Value> = known
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| fields(e["field"].as_str().unwrap()))
        .cloned()
        .collect();
    common::check_verdicts("outer_profiles/known_differences.json", &entries);
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
    test: Option<&'v str>,
    faces: &'v [Value],
    bodies: &'v [Value],
}

fn runs(captured: &Value) -> Vec<Run<'_>> {
    captured["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            assert!(
                r["refused"].is_null(),
                "{}: Python refused the part",
                r["file"]
            );
            Run {
                source: r["source"].as_str().unwrap(),
                file: r["file"].as_str().unwrap(),
                test: r["test"].as_str(),
                faces: r["faces"].as_array().unwrap(),
                bodies: r["bodies"].as_array().unwrap(),
            }
        })
        .collect()
}

fn path(run: &Run, corpus: &Option<PathBuf>) -> Option<PathBuf> {
    Some(match run.source {
        "test" => common::fixtures()
            .join("captured/outer_profiles")
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

/// The runs whose part is available, with the set of their files.
fn available<'r>(runs: &'r [Run<'r>], corpus: &Option<PathBuf>) -> BTreeSet<String> {
    runs.iter()
        .filter(|r| path(r, corpus).is_some())
        .map(|r| r.file.to_string())
        .collect()
}

#[test]
fn outer_profiles_agree_with_python() {
    let captured = captured();
    let runs = runs(&captured);
    let corpus = corpus();
    let files = available(&runs, &corpus);
    let results = common::parallel::map(&runs, |run| {
        let Some(path) = path(run, &corpus) else {
            return (0, 0, Vec::new());
        };
        let part = read_step_file(&path).unwrap_or_else(|e| panic!("{}: {e}", run.file));
        assert_eq!(
            part.faces.len(),
            run.faces.len(),
            "{}: face count",
            run.file
        );
        let mut out = Vec::new();
        let mut profiles = 0;
        for captured in run.faces {
            let face = captured["face"].as_u64().unwrap() as usize;
            if let Ok(p) = planar_outer_profile(&part, face) {
                profiles += 1;
                if let Err(e) = correspondence(&part, &p) {
                    panic!("{} face {face}: {e}", run.file);
                }
            }
            let port = port_face(&part, face);
            let mut python = captured.clone();
            if let Some(body) = captured["body"].as_u64() {
                python["body"] = run.bodies[body as usize].clone();
            }
            for (field, message) in differences(&port, &python) {
                out.push(((run.file.to_string(), field.to_string()), face, message));
            }
        }
        (run.faces.len(), profiles, out)
    });
    let faces: usize = results.iter().map(|r| r.0).sum();
    let profiles: usize = results.iter().map(|r| r.1).sum();
    let found: Vec<Difference> = results.into_iter().flat_map(|r| r.2).collect();
    eprintln!(
        "outer profiles: {faces} captured faces ({profiles} port profiles), {} differ; not \
         captured: {}",
        found.len(),
        captured["skipped"]
    );
    check_known(found, &|f| !f.contains(':'), &files);
}

/// A named motion: rotation rows and translation.
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);

/// Two of `tests/invariance.rs`'s rigid motions (one moving every axis and the origin, one
/// reversing two axes) and a generic rotation (37° about (1, 2, 3)), under which no principal
/// axis stays principal.
fn motions() -> Vec<Motion> {
    let [x, y, z] = [1.0f64, 2.0, 3.0].map(|c| c / 14.0f64.sqrt());
    let (s, c) = 37.0f64.to_radians().sin_cos();
    let t = 1.0 - c;
    vec![
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
        (
            "generic",
            [
                [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
                [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
                [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
            ],
            [19.0, 29.0, 37.0],
        ),
    ]
}

/// The point (or, without the translation, direction) *p* of the moved part, moved back.
fn back(r: &[[f64; 3]; 3], t: [f64; 3], p: [f64; 3], point: bool) -> [f64; 3] {
    let d = if point {
        [0, 1, 2].map(|i| p[i] - t[i])
    } else {
        p
    };
    [0, 1, 2].map(|j| (0..3).map(|i| r[i][j] * d[i]).sum())
}

/// What does not depend on placement: the refusal, or the profile moved back by the motion with
/// its supports from the one that starts where the unmoved profile's first does (the first
/// support is the least vertex, which turns with the axes), and the body and source edges.
fn placement_free(
    part: &Part,
    face: usize,
    motion: Option<(&[[f64; 3]; 3], [f64; 3])>,
    anchor: Option<[f64; 3]>,
) -> Value {
    let p = match planar_outer_profile(part, face) {
        Err(r) => return json!({"refused": r.reason.python_value()}),
        Ok(p) => p,
    };
    let map = |q: [f64; 3], point: bool| match motion {
        Some((r, t)) => back(r, t, q, point),
        None => q,
    };
    let supports: Vec<Value> = p
        .profile
        .supports
        .iter()
        .map(|s| match s {
            ProfileSupport::Line(l) => json!([map(l.start, true), map(l.end, true)]),
            ProfileSupport::Arc(a) => json!([
                map(a.start, true),
                map(a.end, true),
                map(a.center, true),
                a.radius,
                a.sweep
            ]),
        })
        .collect();
    let n = supports.len();
    let shift = anchor
        .and_then(|a| {
            (0..n).find(|&i| {
                let s = map(p.profile.supports[i].start(), true);
                (0..3).all(|k| (s[k] - a[k]).abs() <= 1e-6 * a[k].abs().max(1.0))
            })
        })
        .unwrap_or(0);
    let order =
        |v: &[Value]| -> Vec<Value> { (0..n).map(|i| v[(i + shift) % n].clone()).collect() };
    let edges: Vec<Value> = p.edges.iter().map(|&e| json!(e)).collect();
    json!({
        "normal": map(p.profile.normal, false),
        "inner_loop_count": p.profile.inner_loop_count,
        "schema_version": p.profile.schema_version,
        "supports": order(&supports),
        "edges": order(&edges),
        "body": p.body_faces,
    })
}

fn first_start(v: &Value) -> Option<[f64; 3]> {
    let s = &v["supports"][0][0];
    s.is_array()
        .then(|| [0, 1, 2].map(|i| s[i].as_f64().unwrap()))
}

#[test]
fn outer_profiles_are_placement_independent() {
    let captured = captured();
    let runs = runs(&captured);
    let corpus = corpus();
    let files = available(&runs, &corpus);
    let motions = motions();
    let results = common::parallel::map(&runs, |run| {
        let Some(path) = path(run, &corpus) else {
            return Vec::new();
        };
        let base = read_step_file(&path).unwrap();
        let want: Vec<Value> = (0..base.faces.len())
            .map(|f| placement_free(&base, f, None, None))
            .collect();
        let mut out = Vec::new();
        for (motion, r, t) in &motions {
            let placement: Placement = [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]]);
            let moved = read_step_file_placed(&path, &placement).unwrap();
            for (face, want) in want.iter().enumerate() {
                let got = placement_free(&moved, face, Some((r, *t)), first_start(want));
                let field = match (got["refused"].is_null(), want["refused"].is_null()) {
                    (true, true) if !common::same(&got, want) => "profile",
                    (true, true) => continue,
                    _ if got == *want => continue,
                    _ => "outcome",
                };
                out.push((
                    (run.file.to_string(), format!("{motion}:{field}")),
                    face,
                    format!("moved {got}, unmoved {want}"),
                ));
            }
        }
        out
    });
    let found: Vec<Difference> = results.into_iter().flatten().collect();
    eprintln!("outer profiles under motion: {} differ", found.len());
    check_known(found, &|f| f.contains(':'), &files);
}

/// The applicable cases of quiddity's `tests/test_planar_outer_profiles.py`, asserted on the
/// parts each test built (captured as STEP) rather than against Python's answers. Not ported:
/// the evidence view's laziness, caching, authority and serialisation guards (the port's
/// evidence is a plain value of the part, not an issued run-local binding), the framed view
/// (recognition in the part's own frame is `correspondence::recognise_placed`'s, not this
/// query's), and two solids sharing one shell, which STEP export writes as two shells.
#[test]
fn python_outer_profile_cases() {
    let captured = captured();
    let runs = runs(&captured);
    let mut tests_seen = BTreeSet::new();
    for run in runs.iter().filter(|r| r.source == "test") {
        let test = run.test.unwrap().rsplit("::").next().unwrap();
        tests_seen.insert(test);
        let part = read_step_file(&path(run, &None).unwrap()).unwrap();
        let all: Vec<_> = (0..part.faces.len())
            .map(|f| (f, planar_outer_profile(&part, f)))
            .collect();
        let profiles: Vec<&PlanarOuterProfileEvidence> =
            all.iter().filter_map(|(_, p)| p.as_ref().ok()).collect();
        // The cap facing +Z (`cap`).
        let cap = || {
            *profiles
                .iter()
                .find(|p| p.profile.normal[2] > 0.999)
                .unwrap_or_else(|| panic!("{test}: no +Z cap"))
        };
        let reasons: Vec<OuterProfileRefusalReason> = all
            .iter()
            .filter_map(|(_, p)| p.as_ref().err().map(|r| r.reason))
            .collect();
        let lines = |p: &PlanarOuterProfileEvidence| {
            p.profile
                .supports
                .iter()
                .filter(|s| matches!(s, ProfileSupport::Line(_)))
                .count()
        };
        let arcs = |p: &PlanarOuterProfileEvidence| -> Vec<f64> {
            p.profile
                .supports
                .iter()
                .filter_map(|s| match s {
                    ProfileSupport::Arc(a) => Some(a.radius),
                    _ => None,
                })
                .collect()
        };
        match test {
            "test_sharp_and_rounded_profiles_have_known_directed_supports" => {
                for p in profiles
                    .iter()
                    .filter(|p| p.profile.normal[2].abs() > 0.999)
                {
                    assert_eq!(p.profile.inner_loop_count, 0);
                    assert_eq!(lines(p), 3, "{test}");
                    assert!(matches!(p.profile.supports.len(), 3 | 6));
                    assert_eq!(p.profile.schema_version, 1);
                    assert_eq!(p.body_faces.len(), part.faces.len());
                    assert!(p.body_faces.contains(&p.face));
                    for s in &p.profile.supports {
                        if let ProfileSupport::Arc(a) = s {
                            assert!((a.sweep - std::f64::consts::TAU / 3.0).abs() < 1e-9);
                        }
                    }
                }
            }
            "test_inner_loop_is_counted_but_never_becomes_outer_adjacency" => {
                let p = cap();
                assert_eq!(p.profile.inner_loop_count, 1);
                assert_eq!(p.profile.supports.len(), 6);
                let outer = part.outer_loop(p.face).unwrap();
                for (at, lp) in part.faces[p.face].loops.iter().enumerate() {
                    if at != outer {
                        assert!(lp.edges.iter().all(|(e, _)| !p.edges.contains(e)));
                    }
                }
            }
            "test_concave_profile_is_preserved_and_nonplanar_face_refuses" => {
                let p = cap();
                assert_eq!(p.profile.supports.len(), 6);
                assert_eq!(p.profile.schema_version, 2);
            }
            "test_profile_made_only_of_finite_arcs_is_preserved" => {
                let p = cap();
                assert_eq!(p.profile.supports.len(), 2);
                assert_eq!(arcs(p).len(), 2);
                assert_eq!(p.profile.schema_version, 2);
                assert!(reasons.contains(&OuterProfileRefusalReason::NotPlanar));
            }
            "test_mixed_freeform_wire_is_not_replaced_with_straight_supports" => {
                assert!(reasons.contains(&OuterProfileRefusalReason::UnsupportedCurve));
                assert!(profiles.iter().all(|p| p.profile.normal[2].abs() < 0.999));
            }
            "test_equal_and_touching_bodies_never_share_profile_identity" => {
                let caps: Vec<_> = profiles
                    .iter()
                    .filter(|p| p.profile.normal[2] > 0.999)
                    .collect();
                assert_eq!(caps.len(), 2);
                assert!(
                    caps[0]
                        .body_faces
                        .iter()
                        .all(|f| !caps[1].body_faces.contains(f))
                );
                assert!(
                    caps.iter()
                        .all(|p| p.body_faces.len() == 6 && p.profile.supports.len() == 4)
                );
            }
            "test_shared_topology_and_unowned_face_refuse_instead_of_guessing_body" => {
                assert_eq!(reasons, [OuterProfileRefusalReason::AmbiguousBody]);
            }
            "test_loose_vertex_tolerance_cannot_invent_a_different_line_support" => {
                let top = all
                    .iter()
                    .find(|(f, _)| part.face_normal(*f, 0.0, 0.0).unwrap()[2] > 0.999)
                    .unwrap();
                assert_eq!(
                    top.1.as_ref().unwrap_err().reason,
                    OuterProfileRefusalReason::InvalidBoundary
                );
            }
            "test_six_line_flange_with_alternating_arc_radii_and_holes" => {
                let p = cap();
                assert_eq!(p.profile.supports.len(), 12);
                assert_eq!(p.profile.inner_loop_count, 3);
                assert_eq!(lines(p), 6);
                let mut radii = arcs(p);
                radii.sort_by(f64::total_cmp);
                for (r, want) in radii.iter().zip([3.0, 3.0, 3.0, 8.0, 8.0, 8.0]) {
                    assert!((r - want).abs() < 1e-6, "{radii:?}");
                }
            }
            "test_nist_import_never_publishes_displaced_vertex_as_a_trimmed_support" => {
                assert!(!profiles.is_empty());
                assert!(reasons.contains(&OuterProfileRefusalReason::InvalidBoundary));
            }
            "test_raw_rigid_motion_preserves_profile_supports_and_source_binding"
            | "test_framed_profile_maps_to_exact_caller_face_without_another_run"
            | "test_step_reimport_preserves_geometry_with_fresh_source_binding" => {
                // A rounded triangle, moved: two six-support caps.
                let six: Vec<_> = profiles
                    .iter()
                    .filter(|p| p.profile.supports.len() == 6)
                    .collect();
                assert_eq!(six.len(), 2, "{test}");
            }
            other => panic!("unexpected captured test {other}"),
        }
    }
    assert_eq!(tests_seen.len(), 13, "{tests_seen:?}");
}

/// `test_rounded_transition_supplies_an_explicit_virtual_intersection` and
/// `test_opposite_normal_and_reversed_wire_keep_physical_support_correspondence`, on the rounded
/// triangle: each arc's neighbouring lines, extended, meet 50 from the axis at no support's
/// vertex; and the same face reversed has the opposite normal and the same supports reversed.
#[test]
fn rounded_triangle_corners_and_reversed_face() {
    let captured = captured();
    let runs = runs(&captured);
    let run = runs
        .iter()
        .find(|r| {
            r.test.is_some_and(|t| {
                t.ends_with("test_step_reimport_preserves_geometry_with_fresh_source_binding")
            })
        })
        .unwrap();
    let part = read_step_file(&path(run, &None).unwrap()).unwrap();
    let (face, p) = (0..part.faces.len())
        .find_map(|f| {
            planar_outer_profile(&part, f)
                .ok()
                .filter(|p| p.profile.normal[2] > 0.999)
                .map(|p| (f, p))
        })
        .unwrap();
    let s = &p.profile.supports;
    let mut vertices = Vec::new();
    for (at, arc) in s.iter().enumerate() {
        if !matches!(arc, ProfileSupport::Arc(_)) {
            continue;
        }
        let (ProfileSupport::Line(left), ProfileSupport::Line(right)) =
            (&s[(at + s.len() - 1) % s.len()], &s[(at + 1) % s.len()])
        else {
            panic!("an arc between two lines");
        };
        let (a, b) = (left.direction(), right.direction());
        let delta: Vec<f64> = (0..3).map(|i| right.start[i] - left.end[i]).collect();
        let t = (delta[0] * b[1] - delta[1] * b[0]) / (a[0] * b[1] - a[1] * b[0]);
        let vertex: Vec<f64> = (0..3).map(|i| left.end[i] + t * a[i]).collect();
        assert!(t > 0.0);
        assert!(
            (0..3)
                .map(|i| (vertex[i] - right.start[i]) * b[i])
                .sum::<f64>()
                < 0.0
        );
        assert!((vertex[0].hypot(vertex[1]) - 50.0).abs() < 1e-6);
        assert!(s.iter().all(|q| {
            (0..3)
                .map(|i| (vertex[i] - q.start()[i]).powi(2))
                .sum::<f64>()
                .sqrt()
                >= 1e-6
        }));
        vertices.push(vertex);
    }
    assert_eq!(vertices.len(), 3);

    // The same part with this face reversed (its wire and solid untouched).
    let mut faces = part.faces.clone();
    faces[face].reversed = !faces[face].reversed;
    let flipped = Part::new(faces, part.edges.clone(), part.solids.clone());
    let q = planar_outer_profile(&flipped, face).unwrap();
    assert!((0..3).all(|i| (q.profile.normal[i] + p.profile.normal[i]).abs() < 1e-12));
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6);
    for support in s {
        assert!(
            q.profile
                .supports
                .iter()
                .any(|t| near(support.start(), t.end()) && near(support.end(), t.start()))
        );
    }
}
