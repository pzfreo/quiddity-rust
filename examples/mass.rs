//! Per solid of each STEP file: volume, area and box, as JSON lines (compare with OpenCascade's).

fn main() {
    for name in std::env::args().skip(1) {
        let line = match quiddity::read_step_file(std::path::Path::new(&name)) {
            Err(e) => serde_json::json!({"file": name, "error": e.to_string()}),
            Ok(part) => {
                let solids: Vec<_> = (0..part.solids.len())
                    .map(|s| {
                        let b = part.solid_bounds(s);
                        let mass = part.solid_mass(s);
                        serde_json::json!({
                            "volume": mass.map(|m| m.0),
                            "area": mass.map(|m| m.1),
                            "bounds": [b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]],
                            "valid": part.solid_is_valid(s),
                        })
                    })
                    .collect();
                serde_json::json!({"file": name, "solids": solids})
            }
        };
        println!("{line}");
    }
}
