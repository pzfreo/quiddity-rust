//! Signed parameter-space area of each face's loops, normalised by face orientation:
//! positive means the loop has the face on its left. `cargo run --example orient -- file.step`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&args[1])).expect("read");
    let (mut pos, mut neg) = (0, 0);
    for i in 0..part.faces.len() {
        let f = &part.faces[i];
        let Some(loops) = part.uv_loops(i) else {
            continue;
        };
        let mut areas = Vec::new();
        for lp in loops {
            let p = &lp.points;
            if lp.degenerate || p.len() < 3 {
                continue;
            }
            let closed = (p[0].0 - p[p.len() - 1].0).abs() < 1e-6
                && (p[0].1 - p[p.len() - 1].1).abs() < 1e-6;
            if !closed {
                continue;
            }
            let a: f64 = p
                .windows(2)
                .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
                .sum::<f64>()
                * 0.5;
            areas.push(a * if f.reversed { -1.0 } else { 1.0 });
        }
        // The outer loop is the one with the largest magnitude.
        if let Some(outer) = areas
            .iter()
            .copied()
            .max_by(|a, b| a.abs().total_cmp(&b.abs()))
        {
            if outer > 0.0 {
                pos += 1
            } else {
                neg += 1;
                if args.len() > 2 {
                    println!(
                        "face {i} {:?} rev={} outer={outer:.4}",
                        f.surface.kind(),
                        f.reversed
                    );
                }
            }
        }
    }
    println!("{}: face-on-left {pos}, face-on-right {neg}", args[1]);
}
