//! Solid centroid and inferred frame for live independent-reference checks.
fn main() {
    for file in std::env::args().skip(1) {
        let part = quiddity::read_step_file(std::path::Path::new(&file)).expect("read STEP");
        let frame = quiddity::frames::infer_part_frame(&part).map_err(|reason| reason.as_str());
        println!(
            "{}",
            serde_json::json!({
                "file": file, "centroid": part.volume_centroid(), "frame": frame,
                "valid": (0..part.solids.len()).map(|i| part.solid_is_valid(i)).collect::<Vec<_>>(),
            })
        );
    }
}
