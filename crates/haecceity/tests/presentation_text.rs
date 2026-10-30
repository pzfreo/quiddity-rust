#![cfg(feature = "presentation")]
//! draftwright's text against build123d's `Text` (`tools/capture_text.py`): every sample label
//! in both of draftwright's fonts must have build123d's outline (within `common::TOL` both ways),
//! bounding box and area.

#[path = "common/presentation_text.rs"]
mod common;

#[test]
fn text_matches_build123d() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/presentation_text.json.gz");
    let mut problems = Vec::new();
    for case in common::load(&path)["cases"].as_array().unwrap() {
        let s = common::compare(case);
        if s.outline.0 < 0.999
            || s.outline.1 < 0.999
            || s.box_error > 5e-5
            || ((s.area.1 - s.area.0) / s.area.0).abs() > 2e-4
        {
            problems.push(format!(
                "{} {:?}: box {:.1e} outline {:.4} {:.4} area {} {}",
                case["font"],
                case["text"],
                s.box_error,
                s.outline.0,
                s.outline.1,
                s.area.0,
                s.area.1
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
