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
