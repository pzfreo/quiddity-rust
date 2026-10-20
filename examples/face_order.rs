//! Geometry signatures for live OpenCascade face-order verification (not Python captures).
//! `cargo run --release --example face_order -- STEPS_DIR IDS_FILE > signatures.jsonl`
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let steps = std::path::Path::new(&args[1]);
    let out = std::io::stdout();
    let mut out = out.lock();
    for id in std::fs::read_to_string(&args[2])?.lines() {
        let part = quiddity::read_step_file(&steps.join(format!("{id}.step")))?;
        let faces: Vec<_> = part.faces.iter().enumerate().map(|(i, face)| {
            let m = part.face_moments(i).expect("face moments");
            serde_json::json!({"kind": format!("{:?}", face.surface.kind()), "area":m.area, "centroid":m.centroid})
        }).collect();
        writeln!(out, "{}", serde_json::json!({"id":id,"faces":faces}))?;
    }
    Ok(())
}
