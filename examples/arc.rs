//! The arc between two faces, edge by edge: `-- file a b`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&a[1])).expect("read");
    let (f, g): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    for e in part.shared_edges(f, g) {
        let ed = &part.edges[e];
        println!(
            "edge {e} closed={} {:?} -> {:?} curve={:?}",
            ed.is_closed(),
            ed.start,
            ed.end,
            std::mem::discriminant(&ed.curve)
        );
        for (face, lp) in [f, g]
            .iter()
            .flat_map(|&x| part.faces[x].loops.iter().map(move |l| (x, l)))
        {
            if let Some(u) = lp.edges.iter().find(|u| u.0 == e) {
                println!("  face {face} uses forward={}", u.1);
            }
        }
    }
    println!("arc {:?} / {:?}", part.arc(f, g), part.arc(g, f));
    if let Some(&e) = part.shared_edges(f, g).first() {
        let s = &part.edges[e].samples;
        let p = s[s.len() / 2];
        for x in [f, g] {
            let (u, v) = part.faces[x].surface.parameters(p, None).unwrap();
            println!("face {x} normal {:?} at {:?}", part.face_normal(x, u, v), part.faces[x].surface.value(u, v));
        }
    }
}
