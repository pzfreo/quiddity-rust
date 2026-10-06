//! Print an edge's curve and samples near its ends: `cargo run --example edge -- file.step edge`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&args[1])).expect("read");
    let e = &part.edges[args[2].parse::<usize>().unwrap()];
    println!("{:?} -> {:?} same_sense={}", e.start, e.end, e.same_sense);
    if let quiddity::geom::Curve::Nurbs(n) = &e.curve {
        let (lo, hi) = n.domain();
        println!(
            "domain ({lo},{hi}) value(lo)={:?} value(hi)={:?}",
            n.value(lo),
            n.value(hi)
        );
        println!("t(start)={} t(end)={}", n.invert(e.start), n.invert(e.end));
    }
    for p in e.samples.iter().take(4) {
        println!("  {p:?}");
    }
}
