//! Print the cylinder inventory: `cargo run --example cylinders -- file.step`.
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let part = quiddity::read_step_file(std::path::Path::new(&path)).expect("read");
    for c in quiddity::features::cylinders::analyse_cylinders(&part) {
        println!(
            "face {:3} solid {} d={} axis={} u={:.6} ap={:?} dir={:?} s=[{}, {}] ext={}",
            c.face,
            c.solid,
            c.diameter,
            c.axis,
            c.u_extent,
            c.axis_point,
            c.direction,
            c.s_lo,
            c.s_hi,
            c.external
        );
    }
}
