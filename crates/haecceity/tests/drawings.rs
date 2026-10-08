//! Hidden-line projections and section views against OpenCascade's, as draftwright draws them
//! (`tools/capture_hlr.py`, `tools/capture_section.py`). A view agrees when at least `AGREE` of
//! each side's visible lines, and of its hidden lines as inked, lie within `drawing::TOL` of the
//! other's; a section's cut agrees when its outline does alike and the areas the two outlines
//! bound (by the even-odd rule, as draftwright hatches) agree within `AREA`. Every view or cut
//! that does not is listed in `known_drawings.json`, with its verdict and how far it may be off:
//! its lowest score (`min_score`) and, for a cut, its area's relative error (`max_area_error`),
//! so a listed difference that gets worse still fails. A part some of whose curved edge
//! stretches are not drawn with exact curves would be listed there too (its view `exact`), with
//! the share that are (`min_exact_share`), so that a part drawing fewer fails; none is.
//!
//! A view listed as OpenCascade approximating projected curves carries the evidence as
//! `approximation`, checked on every run: every stretch either side draws unlike the other
//! (`drawing::Differences`) is OpenCascade's line off every projected edge of the part, or its
//! hidden line on an edge left inked where its own visible line misses it, or haecceity's line
//! on a projected edge, the two sides at most `stray` apart; haecceity's edges there lie on
//! their faces within `edges_on_faces`; and at most `other` mm of the stretches are of none of
//! these kinds (each explained in the entry's reason).

mod common;

use common::drawing::{
    classified_area, compare_cut, compare_views, covered, differences, even_odd_area, load_gz,
};
use haecceity::read_step_file;
use serde_json::Value;

const AGREE: f64 = 0.99;
const AREA: f64 = 1e-3;
/// Drawn with exact curves, of every `exact_curves_follow_their_points` edge stretch: the share
/// measured when the floor was set (30,293 of 33,464 over every part, once zero-length stretches
/// were no longer drawn), so fewer exact curves fail.
const EXACT_SHARE: f64 = 0.9052;

/// A view or cut that does not agree: its lowest score, its cut area's relative error, and a
/// description.
struct Problem {
    file: String,
    view: String,
    score: f64,
    area_error: f64,
    text: String,
}

#[test]
fn drawings_match_opencascade() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let known: Vec<Value> = serde_json::from_value(common::load("known_drawings.json")).unwrap();
    common::check_verdicts("known_drawings.json", &known);
    let fixtures = ["hlr.json.gz", "section.json.gz"].map(|f| load_gz(&common::fixtures().join(f)));
    let records: Vec<&Value> = fixtures
        .iter()
        .flat_map(|all| all["parts"].as_array().unwrap())
        .collect();
    // Every record spread over every core; the problems come back in fixture order, with any
    // listed approximation the evidence does not bear out.
    let found: Vec<(Vec<Problem>, Vec<String>)> = common::parallel::map(&records, |record| {
        let file = record["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        let mut out = Vec::new();
        let mut unexplained = Vec::new();
        if let Some((theirs, mine)) = compare_cut(record, &part) {
            let outline = (covered(&theirs, &mine), covered(&mine, &theirs));
            let (area, occ_area) = (even_odd_area(&mine), even_odd_area(&theirs));
            let area_error = (area - occ_area).abs() / occ_area;
            if outline.0 < AGREE || outline.1 < AGREE || area_error > AREA {
                out.push(Problem {
                    file: file.to_string(),
                    view: "cut".to_string(),
                    score: outline.0.min(outline.1),
                    area_error,
                    text: format!(
                        "outline {:.4} {:.4}, area {area:.4} against {occ_area:.4}",
                        outline.0, outline.1
                    ),
                });
            }
        }
        for view in compare_views(record, &part) {
            let listed = known
                .iter()
                .find(|k| k["file"] == file && k["view"] == view.name.as_str());
            if let Some(a) = listed.and_then(|k| k.get("approximation")) {
                let d = differences(record, &part, &view);
                let e = d.explained();
                let bound = |key: &str| a[key].as_f64().unwrap_or(0.0);
                if e.stray > bound("stray")
                    || e.other > bound("other")
                    || d.edges_from_faces > bound("edges_on_faces")
                {
                    unexplained.push(format!(
                        "{file} {}: stray {:.2e}, other {:.3} mm, edges on faces {:.1e} \
                         (OpenCascade off the edges by up to {:.2e}), against {a}",
                        view.name, e.stray, e.other, d.edges_from_faces, e.off_edges
                    ));
                }
            }
            let s = view.scores();
            let scores = [s.visible.0, s.visible.1, s.ink.0, s.ink.1];
            if scores.iter().any(|v| *v < AGREE) {
                out.push(Problem {
                    file: file.to_string(),
                    view: view.name.clone(),
                    score: scores.into_iter().fold(1.0, f64::min),
                    area_error: 0.0,
                    text: format!(
                        "visible {:.4} {:.4}, hidden ink {:.4} {:.4}",
                        s.visible.0, s.visible.1, s.ink.0, s.ink.1
                    ),
                });
            }
        }
        (out, unexplained)
    });
    let mut problems: Vec<Problem> = Vec::new();
    let mut failures = Vec::new();
    for (p, u) in found {
        problems.extend(p);
        failures.extend(
            u.into_iter()
                .map(|u| format!("approximation not borne out: {u}")),
        );
    }
    // The exact-curve floors are `exact_curves_follow_their_points`' own.
    let known: Vec<&Value> = known.iter().filter(|k| k["view"] != "exact").collect();
    let listed = |p: &Problem, k: &Value| k["file"] == p.file.as_str() && k["view"] == p.view;
    for p in &problems {
        let line = format!("{} {}: {}", p.file, p.view, p.text);
        let Some(k) = known.iter().find(|k| listed(p, k)) else {
            failures.push(format!("unexpected: {line}"));
            continue;
        };
        let min_score = k["min_score"]
            .as_f64()
            .unwrap_or_else(|| panic!("known_drawings.json: no min_score in {k}"));
        // A cut's area error is relative to OpenCascade's area: `null` where its boolean drew
        // none (the error is then infinite).
        let max_area_error = match (p.view.as_str(), k.get("max_area_error")) {
            ("cut", Some(Value::Null)) => f64::INFINITY,
            ("cut", Some(v)) if v.is_f64() => v.as_f64().unwrap(),
            ("cut", _) => panic!("known_drawings.json: no max_area_error in {k}"),
            _ => 0.0,
        };
        if p.score < min_score || p.area_error > max_area_error {
            failures.push(format!(
                "worse than listed (min_score {min_score}, max_area_error {max_area_error}): {line}"
            ));
        }
    }
    for k in &known {
        if !problems.iter().any(|p| listed(p, k)) {
            failures.push(format!("stale: {k}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A cut OpenCascade's boolean did not make (its `max_area_error` null) is checked against
/// another reference, listed as its `reference_area`: what OpenCascade's own sections of the
/// part by planes 1e-4 to either side have in common, the material on both sides of the plane
/// (`tools/drawing_evidence.py cut`). haecceity's traced cut must lie within
/// `max_reference_error` of it (relative), and the part classified on a grid across the plane
/// must bear the reference out, within the cells the outline crosses or the classifier cannot
/// call (`drawing::classified_area`).
#[test]
fn failed_cuts_match_their_reference() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let known: Vec<Value> = serde_json::from_value(common::load("known_drawings.json")).unwrap();
    let failed: Vec<&Value> = known
        .iter()
        .filter(|k| k["view"] == "cut" && k.get("max_area_error") == Some(&Value::Null))
        .collect();
    let all = load_gz(&common::fixtures().join("section.json.gz"));
    let mut problems = Vec::new();
    for k in &failed {
        let record = all["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["file"] == k["file"])
            .unwrap_or_else(|| panic!("no section record for {k}"));
        let reference = k["reference_area"]
            .as_f64()
            .unwrap_or_else(|| panic!("known_drawings.json: no reference_area in {k}"));
        let allowed = k["max_reference_error"]
            .as_f64()
            .unwrap_or_else(|| panic!("known_drawings.json: no max_reference_error in {k}"));
        let part = read_step_file(&dir.join(k["file"].as_str().unwrap())).unwrap();
        let (_, mine) = compare_cut(record, &part).unwrap();
        let area = even_odd_area(&mine);
        let error = (area - reference).abs() / reference;
        let (classified, doubt) = classified_area(&part, record, &mine, 100);
        println!(
            "{}: traced {area:.4} against {reference:.4} ({error:.1e}); classified {classified:.2} ± {doubt:.2}",
            k["file"]
        );
        if error > allowed || (classified - reference).abs() > doubt {
            problems.push(format!(
                "{}: traced {area:.4}, classified {classified:.2} ± {doubt:.2}, against {reference:.4} \
                 (error {error:.2e}, at most {allowed:.2e})",
                k["file"]
            ));
        }
    }
    assert!(!failed.is_empty() && problems.is_empty(), "{problems:#?}");
}

/// Every stretch drawn with an exact curve is that curve: its ends meet the stretch's (cut on a
/// chord), and the curve between them lies along the stretch's points (within their chords'
/// 0.2 µm tolerance). At least `EXACT_SHARE` of the edge stretches are drawn with exact curves,
/// and of each part's, every one that is not straight (a circle or an ellipse seen edge-on is a
/// segment, drawn by its points) but where `known_drawings.json` lists a lower share for the part
/// (as its view `exact`, the share in `min_exact_share`).
#[test]
fn exact_curves_follow_their_points() {
    use haecceity::hlr::{Class, View, project};
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let known: Vec<Value> = serde_json::from_value(common::load("known_drawings.json")).unwrap();
    let floors: Vec<&Value> = known.iter().filter(|k| k["view"] == "exact").collect();
    let all = load_gz(&common::fixtures().join("hlr.json.gz"));
    let records: Vec<&Value> = all["parts"].as_array().unwrap().iter().collect();
    // Every record spread over every core; the counts and problems come back in fixture order.
    let found = common::parallel::map(&records, |record| {
        let (mut exact, mut edges, mut problems) = (0, 0, Vec::new());
        // The stretches that are not straight, and how many of them are exact.
        let (mut curved, mut curved_exact) = (0, 0);
        let file = record["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        for (toward, up) in [
            ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([1.0, -1.0, 1.0], [0.0, 0.0, 1.0]),
        ] {
            let view = View::new(toward, up).unwrap();
            for piece in project(&part, &view).unwrap() {
                let pts = &piece.points;
                // Every stretch draws something: none is shorter than the 1e-9 of the part's
                // size below which a stretch is passed over.
                let length: f64 = pts
                    .windows(2)
                    .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                    .sum();
                if length <= 1e-9
                    || piece
                        .exact
                        .as_ref()
                        .is_some_and(|c| c.range().0 == c.range().1)
                {
                    problems.push(format!("{file} {toward:?}: a stretch of no length {pts:?}"));
                }
                if piece.class != Class::Outline {
                    edges += 1;
                    if !straight(pts) {
                        curved += 1;
                        curved_exact += usize::from(piece.exact.is_some());
                    }
                }
                let Some(curve) = &piece.exact else { continue };
                exact += 1;
                let (t0, t1) = curve.range();
                let gap = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
                let ends = gap(curve.at(t0), pts[0]).max(gap(curve.at(t1), pts[pts.len() - 1]));
                let along = (0..=32)
                    .map(|k| {
                        let q = curve.at(t0 + (t1 - t0) * k as f64 / 32.0);
                        pts.windows(2)
                            .map(|w| segment_distance(q, w[0], w[1]))
                            .fold(f64::INFINITY, f64::min)
                    })
                    .fold(0.0, f64::max);
                if ends > 3e-4 || along > 3e-4 {
                    problems.push(format!(
                        "{file} {toward:?}: {curve:?} ends {ends:.1e} along {along:.1e}"
                    ));
                }
            }
        }
        let floor = floors.iter().find(|k| k["file"] == file);
        let line = format!("{file}: {curved_exact} of {curved} curved edge stretches exact");
        match floor.map(|k| k["min_exact_share"].as_f64()) {
            None if curved_exact < curved => problems.push(line),
            None => {}
            Some(None) => panic!("known_drawings.json: no min_exact_share in {floor:?}"),
            Some(Some(_)) if curved_exact == curved => problems.push(format!("stale: {line}")),
            Some(Some(min)) if (curved_exact as f64) < min * curved as f64 => {
                problems.push(format!("{line}, below the floor of {min}"))
            }
            Some(Some(_)) => {}
        }
        (exact, edges, problems)
    });
    let (mut exact, mut edges, mut problems) = (0, 0, Vec::new());
    for (x, e, p) in found {
        exact += x;
        edges += e;
        problems.extend(p);
    }
    for k in &floors {
        if !records.iter().any(|r| r["file"] == k["file"]) {
            problems.push(format!("stale: {k}"));
        }
    }
    println!("{exact} of {edges} edge stretches exact");
    assert!(
        exact as f64 >= EXACT_SHARE * edges as f64,
        "{exact} of {edges} edge stretches exact, below the floor of {EXACT_SHARE}"
    );
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Whether the points lie along one line (within the exact curves' 3e-4 tolerance): measured
/// from the line through the first and the point furthest from it, as a stretch seen edge-on
/// can run there and back.
fn straight(pts: &[[f64; 2]]) -> bool {
    let gap = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let far = pts
        .iter()
        .copied()
        .max_by(|p, q| gap(*p, pts[0]).total_cmp(&gap(*q, pts[0])))
        .unwrap_or(pts[0]);
    let length = gap(far, pts[0]);
    length == 0.0
        || pts.iter().all(|p| {
            let cross = (far[0] - pts[0][0]) * (p[1] - pts[0][1])
                - (far[1] - pts[0][1]) * (p[0] - pts[0][0]);
            cross.abs() / length <= 3e-4
        })
}

fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}
