//! A face's pcurves and per-edge parameter ranges: `-- file face`.
use quiddity::kernel::brep::Pcurve;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&a[1])).expect("read");
    let face: usize = a[2].parse().unwrap();
    let f = &part.faces[face];
    for (e, c) in &f.pcurves {
        match c {
            Pcurve::Poles(p) => {
                let v: Vec<f64> = p.iter().map(|q| q.1).collect();
                let u: Vec<f64> = p.iter().map(|q| q.0).collect();
                println!(
                    "edge {e} poles n={} u[{:.4},{:.4}] v[{:.4},{:.4}]",
                    p.len(),
                    u.iter().cloned().fold(f64::INFINITY, f64::min),
                    u.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    v.iter().cloned().fold(f64::INFINITY, f64::min),
                    v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                );
            }
            Pcurve::Line { point, dir } => println!("edge {e} line {point:?} dir {dir:?}"),
        }
    }
    for e in part.face_edges(face) {
        let ed = &part.edges[e];
        println!(
            "edge {e} {:?} -> {:?} closed={} curve={}",
            ed.start,
            ed.end,
            ed.is_closed(),
            match ed.curve {
                quiddity::kernel::geom::Curve::Nurbs(_) => "nurbs",
                quiddity::kernel::geom::Curve::Line { .. } => "line",
                quiddity::kernel::geom::Curve::Circle { .. } => "circle",
                _ => "other",
            }
        );
    }
    println!("uv_bounds {:?}", part.uv_bounds(face));
}
