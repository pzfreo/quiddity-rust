//! Debug a face's parameter-space domain: `cargo run --example domain -- file.step face`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&args[1])).expect("read");
    let face: usize = args[2].parse().unwrap();
    let f = &part.faces[face];
    println!(
        "{:?} reversed={} uv_bounds={:?}",
        f.surface.kind(),
        f.reversed,
        part.uv_bounds(face)
    );
    for lp in part.uv_loops(face).unwrap() {
        let pts = &lp.points;
        let step = (pts.len() / 12).max(1);
        println!(
            "loop n={}: {:?}",
            pts.len(),
            pts.iter()
                .step_by(step)
                .map(|(u, v)| (format!("{u:.3}"), format!("{v:.3}")))
                .collect::<Vec<_>>()
        );
    }
    let d = part.domain(face).unwrap();
    let ((u0, u1), (v0, v1)) = (d.u_range(), d.v_range());
    for j in 0..=8 {
        let v = v0 - 0.5 + (v1 - v0 + 1.0) * j as f64 / 8.0;
        let row: String = (0..=16)
            .map(|i| {
                let u = u0 + (u1 - u0) * i as f64 / 16.0;
                if d.contains(u, v) { '#' } else { '.' }
            })
            .collect();
        println!("v={v:8.3} {row}");
    }
}
