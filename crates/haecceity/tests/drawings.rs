//! Hidden-line projections and section views against OpenCascade's, as draftwright draws them
//! (`tools/capture_hlr.py`, `tools/capture_section.py`). A view agrees when at least `AGREE` of
//! each side's visible lines, and of its hidden lines as inked, lie within `drawing::TOL` of the
//! other's; a section's cut agrees when its outline does alike and the areas the two outlines
//! bound (by the even-odd rule, as draftwright hatches) agree within `AREA`. Every view or cut
//! that does not is listed in `known_drawings.json`, with its verdict and how far it may be off:
//! its lowest score (`min_score`) and, for a cut, its area's relative error (`max_area_error`),
//! so a listed difference that gets worse still fails.

mod common;

use common::drawing::{compare_cut, compare_views, covered, even_odd_area, load_gz};
use haecceity::read_step_file;
use serde_json::Value;

const AGREE: f64 = 0.99;
const AREA: f64 = 1e-3;
/// Drawn with exact curves, of every `exact_curves_follow_their_points` edge stretch: the share
/// measured when the floor was set (9,927 of 11,105 at c9ce1cd), so fewer exact curves fail.
const EXACT_SHARE: f64 = 0.8939;

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
    // Every record spread over every core; the problems come back in fixture order.
    let problems: Vec<Problem> = common::parallel::map(&records, |record| {
        let file = record["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        let mut out = Vec::new();
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
        out
    })
    .into_iter()
    .flatten()
    .collect();
    let listed = |p: &Problem, k: &Value| k["file"] == p.file.as_str() && k["view"] == p.view;
    let mut failures = Vec::new();
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

/// Every stretch drawn with an exact curve is that curve: its ends meet the stretch's (cut on a chord), and the
/// curve between them lies along the stretch's points (within their chords' 0.2 µm tolerance).
/// And at least `EXACT_SHARE` of the stretches are drawn with exact curves.
#[test]
fn exact_curves_follow_their_points() {
    use haecceity::hlr::{Class, View, project};
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let all = load_gz(&common::fixtures().join("hlr.json.gz"));
    let records: Vec<&Value> = all["parts"].as_array().unwrap().iter().step_by(4).collect();
    // Every record spread over every core; the counts and problems come back in fixture order.
    let found = common::parallel::map(&records, |record| {
        let (mut exact, mut edges, mut problems) = (0, 0, Vec::new());
        let file = record["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        for (toward, up) in [
            ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([1.0, -1.0, 1.0], [0.0, 0.0, 1.0]),
        ] {
            let view = View::new(toward, up).unwrap();
            for piece in project(&part, &view) {
                if piece.class != Class::Outline {
                    edges += 1;
                }
                let Some(curve) = &piece.exact else { continue };
                exact += 1;
                let pts = &piece.points;
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
        (exact, edges, problems)
    });
    let (mut exact, mut edges, mut problems) = (0, 0, Vec::new());
    for (x, e, p) in found {
        exact += x;
        edges += e;
        problems.extend(p);
    }
    println!("{exact} of {edges} edge stretches exact");
    assert!(
        exact as f64 >= EXACT_SHARE * edges as f64,
        "{exact} of {edges} edge stretches exact, below the floor of {EXACT_SHARE}"
    );
    assert!(
        problems.is_empty(),
        "{} stray:\n{}",
        problems.len(),
        problems
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
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
