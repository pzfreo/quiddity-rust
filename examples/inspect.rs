//! Print a STEP file's face inventory: `cargo run --example inspect -- file.step [face...]`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&args[1])).expect("read");
    println!(
        "{} faces, {} edges, {} solids",
        part.faces.len(),
        part.edges.len(),
        part.solids.len()
    );
    let wanted: Vec<usize> = args[2..].iter().filter_map(|a| a.parse().ok()).collect();
    for (i, f) in part.faces.iter().enumerate() {
        if !wanted.is_empty() && !wanted.contains(&i) {
            continue;
        }
        let b = part.face_bounds(i);
        println!(
            "{i}: {:?} rev={} loops={:?} uv={:?} bounds={:?}..{:?}",
            f.surface.kind(),
            f.reversed,
            f.loops.iter().map(|l| l.edges.len()).collect::<Vec<_>>(),
            part.uv_bounds(i),
            b.min,
            b.max
        );
        if !wanted.is_empty() {
            for lp in &f.loops {
                for &(e, fwd) in &lp.edges {
                    let edge = &part.edges[e];
                    let kind = match &edge.curve {
                        quiddity::kernel::geom::Curve::Line { .. } => "line",
                        quiddity::kernel::geom::Curve::Circle { .. } => "circle",
                        quiddity::kernel::geom::Curve::Ellipse { .. } => "ellipse",
                        quiddity::kernel::geom::Curve::Nurbs(_) => "nurbs",
                    };
                    println!(
                        "   edge {e} {kind} fwd={fwd} same_sense={} {:?} -> {:?} ({} samples)",
                        edge.same_sense,
                        edge.start,
                        edge.end,
                        edge.samples.len()
                    );
                }
            }
        }
    }
}
