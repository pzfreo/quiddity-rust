//! Hidden-line projections and section views against OpenCascade's, as draftwright draws them
//! (`tools/capture_hlr.py`, `tools/capture_section.py`). A view agrees when at least `AGREE` of
//! each side's visible lines, and of its hidden lines as inked, lie within `drawing::TOL` of the
//! other's; a section's cut agrees when its outline does alike and the areas the two outlines
//! bound (by the even-odd rule, as draftwright hatches) agree within `AREA`. Every view or cut
//! that does not is listed in `known_drawings.json`, with its verdict.

mod common;

use common::drawing::{compare_cut, compare_views, covered, even_odd_area, load_gz};
use haecceity::read_step_file;
use serde_json::Value;

const AGREE: f64 = 0.99;
const AREA: f64 = 1e-3;

#[test]
fn drawings_match_opencascade() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let known: Vec<Value> = serde_json::from_value(common::load("known_drawings.json")).unwrap();
    common::check_verdicts("known_drawings.json", &known);
    let mut problems = Vec::new();
    for fixture in ["hlr.json.gz", "section.json.gz"] {
        let all = load_gz(&common::fixtures().join(fixture));
        for record in all["parts"].as_array().unwrap() {
            let file = record["file"].as_str().unwrap();
            let part = read_step_file(&dir.join(file)).unwrap();
            if let Some((theirs, mine)) = compare_cut(record, &part) {
                let outline = (covered(&theirs, &mine), covered(&mine, &theirs));
                let (area, occ_area) = (even_odd_area(&mine), even_odd_area(&theirs));
                if outline.0 < AGREE
                    || outline.1 < AGREE
                    || (area - occ_area).abs() > AREA * occ_area
                {
                    problems.push(format!(
                        "{file} cut: outline {:.4} {:.4}, area {area:.4} against {occ_area:.4}",
                        outline.0, outline.1
                    ));
                }
            }
            for view in compare_views(record, &part) {
                let s = view.scores();
                if [s.visible.0, s.visible.1, s.ink.0, s.ink.1]
                    .iter()
                    .any(|v| *v < AGREE)
                {
                    problems.push(format!(
                        "{file} {}: visible {:.4} {:.4}, hidden ink {:.4} {:.4}",
                        view.name, s.visible.0, s.visible.1, s.ink.0, s.ink.1
                    ));
                }
            }
        }
    }
    let listed = |problem: &str, k: &Value| {
        problem.starts_with(&format!(
            "{} {}:",
            k["file"].as_str().unwrap(),
            k["view"].as_str().unwrap()
        ))
    };
    let unexpected: Vec<&String> = problems
        .iter()
        .filter(|p| !known.iter().any(|k| listed(p, k)))
        .collect();
    let stale: Vec<&Value> = known
        .iter()
        .filter(|k| !problems.iter().any(|p| listed(p, k)))
        .collect();
    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "{} unexpected:\n{}\nstale: {stale:?}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every stretch drawn with an exact curve is that curve: its ends meet the stretch's (cut on a chord), and the
/// curve between them lies along the stretch's points (within their chords' 0.2 µm tolerance).
#[test]
fn exact_curves_follow_their_points() {
    use haecceity::hlr::{Class, View, project};
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let all = load_gz(&common::fixtures().join("hlr.json.gz"));
    let (mut exact, mut edges, mut problems) = (0, 0, Vec::new());
    for record in all["parts"].as_array().unwrap().iter().step_by(4) {
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
    }
    println!("{exact} of {edges} edge stretches exact");
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
