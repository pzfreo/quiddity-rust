//! A face's B-spline pcurve end poles against its edges' vertices: `-- file face`.
use quiddity::kernel::brep::Pcurve;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&a[1])).expect("read");
    let f = &part.faces[a[2].parse::<usize>().unwrap()];
    for (e, c) in &f.pcurves {
        if let Pcurve::Poles(p) = c {
            let ed = &part.edges[*e];
            println!(
                "edge {e} start {:?} uv {:?}",
                ed.start,
                f.surface.parameters(ed.start, None)
            );
            println!("  poles {p:?}");
            println!("  first->3d {:?}", f.surface.value(p[0].0, p[0].1));
        }
    }
}
